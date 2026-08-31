use heapless::spsc::Queue;
use static_cell::StaticCell;

pub const SAMPLE_RATE: u32 = 48_000;
pub const BLOCK_SIZE: usize = 512;
pub const AUDIO_BLOCK_COUNT: usize = 3;
pub const AUDIO_QUEUE_SIZE: usize = AUDIO_BLOCK_COUNT + 1;

pub type AudioBlock = [f32; BLOCK_SIZE];
pub type AudioBlockIndex = u8;

const _: () = assert!(AUDIO_BLOCK_COUNT <= AudioBlockIndex::MAX as usize + 1);

static mut AUDIO_BLOCKS: [AudioBlock; AUDIO_BLOCK_COUNT] = [[0.0; BLOCK_SIZE]; AUDIO_BLOCK_COUNT];

pub static FREE_AUDIO_BLOCKS: StaticCell<Queue<AudioBlockIndex, AUDIO_QUEUE_SIZE>> =
	StaticCell::new();
pub static READY_AUDIO_BLOCKS: StaticCell<Queue<AudioBlockIndex, AUDIO_QUEUE_SIZE>> =
	StaticCell::new();

/// Returns the block's first sample. The caller must exclusively own `index`,
/// transferred through the free/ready SPSC token queues, while dereferencing
/// the pointer.
pub unsafe fn block_mut_ptr(index: AudioBlockIndex) -> *mut f32 {
	debug_assert!((index as usize) < AUDIO_BLOCK_COUNT);
	unsafe { (*core::ptr::addr_of_mut!(AUDIO_BLOCKS))[index as usize].as_mut_ptr() }
}

/// Returns the block's first sample. The caller must exclusively own `index`,
/// transferred through the free/ready SPSC token queues, while dereferencing
/// the pointer.
pub unsafe fn block_ptr(index: AudioBlockIndex) -> *const f32 {
	debug_assert!((index as usize) < AUDIO_BLOCK_COUNT);
	unsafe { (*core::ptr::addr_of!(AUDIO_BLOCKS))[index as usize].as_ptr() }
}
