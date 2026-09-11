use defmt::{debug, info, warn};
use embassy_futures::select::{Either, select};
use embassy_rp::Peri;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_time::Instant;
use embassy_usb_driver::host::DeviceEvent;
use embassy_usb_host::class::midi::{MidiHost, event_packets};
use embassy_usb_host::handler::BusRoute;
use embassy_usb_host::{BusState, bus};
use heapless::spsc::Producer;

use crate::midi::MidiEvent;

const MAX_DESCRIPTOR_SIZE: usize = 512;
// A handful of complete 4-byte USB-MIDI event packets per bulk transfer.
const MIDI_TRANSFER_BUFFER_SIZE: usize = 64;
static USB_BUS_STATE: BusState = BusState::new();

fn midi_event(data: &[u8], timestamp_micros: u64) -> Option<MidiEvent> {
    if data.len() < 3 {
        return None;
    }
    let status = data[0];

    match status & 0xf0 {
        0x80 | 0x90 | 0xb0 => Some(MidiEvent {
            status,
            data1: data[1],
            data2: data[2],
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

        let mut midi = match MidiHost::new(&bus, &descriptor_buffer[..descriptor_len], &enum_info) {
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

        if midi.input_ports().is_empty() {
            warn!("MIDI device has no input ports");
            bus.free_address(enum_info.device_address);
            continue;
        }

        info!("USB MIDI input ready");
        let mut transfer_buffer = [0u8; MIDI_TRANSFER_BUFFER_SIZE];
        loop {
            match select(
                midi.read_transfer(&mut transfer_buffer),
                controller.wait_for_device_event(),
            )
            .await
            {
                Either::First(Ok(len)) => {
                    let packets = match event_packets(&transfer_buffer[..len]) {
                        Ok(packets) => packets,
                        Err(error) => {
                            debug!("Ignored malformed USB-MIDI transfer: {:?}", error);
                            continue;
                        }
                    };
                    for packet in packets {
                        let Some(data) = packet.data() else {
                            continue;
                        };
                        if let Some(event) = midi_event(data, Instant::now().as_micros()) {
                            if producer.enqueue(event).is_err() {
                                warn!("MIDI queue full; dropping event");
                            }
                        } else {
                            debug!("Ignoring USB MIDI packet: {=[u8]:x}", data);
                        }
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
