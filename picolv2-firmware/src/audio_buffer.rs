use heapless::spsc::Queue;
use static_cell::StaticCell;

pub const SAMPLE_RATE: u32 = 48_000;
pub const BLOCK_SIZE: usize = 512;
// Roughly one second's worth of blocks, used to throttle diagnostic logging.
// Only referenced when the perf-diagnostics feature is enabled.
#[cfg_attr(not(feature = "perf-diagnostics"), allow(dead_code))]
pub const REPORT_BLOCKS: u32 = (SAMPLE_RATE as usize / BLOCK_SIZE) as u32;
pub const AUDIO_BLOCK_COUNT: usize = 3;
pub const AUDIO_QUEUE_SIZE: usize = AUDIO_BLOCK_COUNT + 1;
pub const I2S_DMA_BUFFER_COUNT: usize = 2;
pub const MIDI_SCHEDULING_DELAY_BLOCKS: usize = AUDIO_BLOCK_COUNT + I2S_DMA_BUFFER_COUNT;

// Interleaved stereo: sample `i`'s left/right channels live at indices
// `2*i` and `2*i + 1`.
pub type AudioBlock = [f32; BLOCK_SIZE * 2];
pub type AudioBlockIndex = u8;

const _: () = assert!(AUDIO_BLOCK_COUNT <= AudioBlockIndex::MAX as usize + 1);

static mut AUDIO_BLOCKS: [AudioBlock; AUDIO_BLOCK_COUNT] =
    [[0.0; BLOCK_SIZE * 2]; AUDIO_BLOCK_COUNT];

pub static FREE_AUDIO_BLOCKS: StaticCell<Queue<AudioBlockIndex, AUDIO_QUEUE_SIZE>> =
    StaticCell::new();
pub static READY_AUDIO_BLOCKS: StaticCell<Queue<AudioBlockIndex, AUDIO_QUEUE_SIZE>> =
    StaticCell::new();

/// Returns a pointer to the block's interleaved stereo samples (L, R, L, R, ...).
/// The caller must exclusively own `index`, transferred through the
/// free/ready SPSC token queues, while dereferencing the pointer.
pub unsafe fn block_mut_ptr(index: AudioBlockIndex) -> *mut f32 {
    debug_assert!((index as usize) < AUDIO_BLOCK_COUNT);
    unsafe { (*core::ptr::addr_of_mut!(AUDIO_BLOCKS))[index as usize].as_mut_ptr() }
}

/// Returns a pointer to the block's interleaved stereo samples (L, R, L, R, ...).
/// The caller must exclusively own `index`, transferred through the
/// free/ready SPSC token queues, while dereferencing the pointer.
pub unsafe fn block_ptr(index: AudioBlockIndex) -> *const f32 {
    debug_assert!((index as usize) < AUDIO_BLOCK_COUNT);
    unsafe { (*core::ptr::addr_of!(AUDIO_BLOCKS))[index as usize].as_ptr() }
}
