use heapless::spsc::Queue;
use static_cell::StaticCell;

pub const SAMPLE_RATE: u32 = 48_000;
pub const BLOCK_SIZE: usize = 512;
pub const AUDIO_QUEUE_SIZE: usize = 3;

pub type AudioBlock = [f32; BLOCK_SIZE];

pub static AUDIO_QUEUE: StaticCell<Queue<AudioBlock, AUDIO_QUEUE_SIZE>> = StaticCell::new();
