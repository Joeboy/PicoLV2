use core::ops::ControlFlow;

use embassy_rp::Peri;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, PIN_18, PIN_19, PIN_20, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};

use super::i2s::{PioI2sOut, PioI2sOutProgram};
use crate::audio::{AudioBlockIndex, BLOCK_SIZE, SAMPLE_RATE, block_ptr};
use crate::diagnostics::{AudioOutputPerformance, diag_info};
use heapless::spsc::{Consumer, Producer};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

const BIT_DEPTH: u32 = 16;

// Pack left and right 16-bit samples into a single u32, as that's what the I2S DMA expects.
#[inline]
fn pack_lr_16(l: i16, r: i16) -> u32 {
    ((l as u32 as u16 as u32) << 16) | ((r as u16) as u32)
}

#[embassy_executor::task]
pub async fn audio_task(
    pio0: Peri<'static, PIO0>,
    dma_ch0: Peri<'static, DMA_CH0>,
    dma_ch1: Peri<'static, DMA_CH1>,
    pin18: Peri<'static, PIN_18>,
    pin19: Peri<'static, PIN_19>,
    pin20: Peri<'static, PIN_20>,
    mut ready_consumer: Consumer<'static, AudioBlockIndex>,
    mut free_producer: Producer<'static, AudioBlockIndex>,
) {
    diag_info!("Starting I2S audio output task");

    let Pio {
        mut common, sm0, ..
    } = Pio::new(pio0, Irqs);

    let bit_clock_pin = pin18;
    let left_right_clock_pin = pin19;
    let data_pin = pin20;

    let program = PioI2sOutProgram::new(&mut common);

    let mut buf_a = [0u32; BLOCK_SIZE];
    let mut buf_b = [0u32; BLOCK_SIZE];

    let mut performance = AudioOutputPerformance::new();

    let mut i2s = PioI2sOut::new(
        &mut common,
        sm0,
        data_pin,
        bit_clock_pin,
        left_right_clock_pin,
        SAMPLE_RATE,
        BIT_DEPTH,
        &program,
    );

    i2s.stream_ping_pong(dma_ch0, dma_ch1, &mut buf_a, &mut buf_b, move |buf| {
        let rendered = if let Some(index) = ready_consumer.dequeue() {
            let samples = unsafe { block_ptr(index) };
            for (sample_index, word) in buf.iter_mut().enumerate() {
                let left = unsafe { samples.add(sample_index * 2).read() };
                let right = unsafe { samples.add(sample_index * 2 + 1).read() };
                let left_pcm = (left * i16::MAX as f32) as i16;
                let right_pcm = (right * i16::MAX as f32) as i16;
                *word = pack_lr_16(left_pcm, right_pcm);
            }
            free_producer
                .enqueue(index)
                .expect("free audio block queue unexpectedly full");
            true
        } else {
            buf.fill(0);
            false
        };
        performance.block_finished(rendered);
        ControlFlow::Continue(())
    })
    .await;
}
