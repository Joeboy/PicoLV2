use defmt::{debug, info, warn};
use embassy_futures::select::{Either, select};
use embassy_rp::Peri;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_time::Instant;
use embassy_usb_driver::host::pipe;
use embassy_usb_driver::host::{DeviceEvent, PipeError, UsbHostAllocator, UsbPipe};
use embassy_usb_driver::{Direction, EndpointInfo, EndpointType};
use embassy_usb_host::descriptor::ConfigurationDescriptorChain;
use embassy_usb_host::handler::{BusRoute, EnumerationInfo, RegisterError};
use embassy_usb_host::{BusState, bus};
use heapless::spsc::Producer;

use crate::midi::MidiEvent;

const MAX_DESCRIPTOR_SIZE: usize = 512;
static USB_BUS_STATE: BusState = BusState::new();

struct MidiHandler<'d, A: UsbHostAllocator<'d>> {
    bulk_in: A::Pipe<pipe::Bulk, pipe::In>,
}

impl<'d, A: UsbHostAllocator<'d>> MidiHandler<'d, A> {
    fn try_register(
        bus: &A,
        enum_info: &EnumerationInfo,
        configuration: &ConfigurationDescriptorChain<'_>,
    ) -> Result<Self, RegisterError> {
        let interface = configuration
            .iter_interface()
            .find(|interface| {
                interface.interface_class == 0x01
                    && interface.interface_subclass == 0x03
                    && interface.interface_protocol == 0x00
            })
            .ok_or(RegisterError::NoSupportedInterface)?;

        let endpoint = interface
            .iter_endpoints()
            .find(|endpoint| {
                endpoint.ep_type() == EndpointType::Bulk && endpoint.ep_dir() == Direction::In
            })
            .ok_or(RegisterError::NoSupportedInterface)?;

        let endpoint: EndpointInfo = endpoint.into();
        let bulk_in = bus.alloc_pipe::<pipe::Bulk, pipe::In>(
            enum_info.device_address,
            &endpoint,
            enum_info.split(),
        )?;

        Ok(Self { bulk_in })
    }

    async fn read_packet(&mut self) -> Result<[u8; 4], PipeError> {
        let mut packet = [0u8; 4];
        self.bulk_in.request_in(&mut packet).await?;
        Ok(packet)
    }
}

fn midi_event(packet: [u8; 4], timestamp_micros: u64) -> Option<MidiEvent> {
    let status = packet[1];

    match status & 0xf0 {
        0x80 | 0x90 | 0xb0 => Some(MidiEvent {
            status,
            data1: packet[2],
            data2: packet[3],
            _reserved: 0,
            timestamp_micros,
        }),
        _ => None,
    }
}

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::host::InterruptHandler<USB>;
});

#[embassy_executor::task]
pub async fn usb_midi_task(
    usb: Peri<'static, USB>,
    mut producer: Producer<'static, MidiEvent>,
) -> ! {
    let driver = embassy_rp::usb::host::Driver::new(usb, Irqs);
    let (mut controller, bus) = bus(driver, &USB_BUS_STATE);

    loop {
        info!("Waiting for USB MIDI device");
        let speed = controller.wait_for_connection().await;
        info!("USB device connected at {:?}", speed);

        let mut descriptor_buffer = [0u8; MAX_DESCRIPTOR_SIZE];
        let (enum_info, descriptor_len) = match bus
            .enumerate(BusRoute::Direct(speed), &mut descriptor_buffer)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                warn!("USB device enumeration failed: {:?}", error);
                continue;
            }
        };

        let configuration = match ConfigurationDescriptorChain::try_from_slice(
            &descriptor_buffer[..descriptor_len],
        ) {
            Ok(configuration) => configuration,
            Err(error) => {
                warn!("Invalid USB configuration descriptor: {:?}", error);
                bus.free_address(enum_info.device_address);
                continue;
            }
        };

        let mut midi = match MidiHandler::try_register(&bus, &enum_info, &configuration) {
            Ok(midi) => midi,
            Err(error) => {
                warn!(
                    "Connected USB device has no supported MIDI input: {:?}",
                    error
                );
                bus.free_address(enum_info.device_address);
                continue;
            }
        };

        info!("USB MIDI input ready");
        loop {
            match select(midi.read_packet(), controller.wait_for_device_event()).await {
                Either::First(Ok(packet)) => {
                    if let Some(event) = midi_event(packet, Instant::now().as_micros()) {
                        if producer.enqueue(event).is_err() {
                            warn!("MIDI queue full; dropping event");
                        }
                    } else {
                        debug!("Ignoring USB MIDI packet: {=[u8]:x}", &packet[..]);
                    }
                }
                Either::First(Err(error)) => {
                    warn!("USB MIDI read failed: {:?}", error);
                    break;
                }
                Either::Second(DeviceEvent::Disconnected) => {
                    info!("USB MIDI device disconnected");
                    break;
                }
                Either::Second(event) => debug!("USB device event: {:?}", event),
            }
        }

        // Prevent stuck notes once the plugin starts maintaining note state.
        let _ = producer.enqueue(MidiEvent {
            status: 0xb0,
            data1: 123,
            data2: 0,
            _reserved: 0,
            timestamp_micros: Instant::now().as_micros(),
        });

        bus.free_address(enum_info.device_address);
    }
}
