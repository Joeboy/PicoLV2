use core::alloc::{GlobalAlloc, Layout};

use embedded_alloc::TlsfHeap as Heap;

#[cfg(picolv2_perf_diagnostics)]
use crate::audio::{BLOCK_SIZE, SAMPLE_RATE};

#[cfg(picolv2_perf_diagnostics)]
const REPORT_BLOCKS: u32 = (SAMPLE_RATE as usize / BLOCK_SIZE) as u32;

use defmt_rtt as _;

macro_rules! diag_info {
    ($($arg:tt)*) => { defmt::info!($($arg)*) };
}

macro_rules! diag_debug {
    ($($arg:tt)*) => { defmt::debug!($($arg)*) };
}

macro_rules! diag_warn {
    ($($arg:tt)*) => { defmt::warn!($($arg)*) };
}

pub(crate) use diag_debug;
pub(crate) use diag_info;
pub(crate) use diag_warn;

pub(crate) struct SystemHeap {
    heap: Heap,
    #[cfg(picolv2_perf_diagnostics)]
    current: core::sync::atomic::AtomicUsize,
    #[cfg(picolv2_perf_diagnostics)]
    peak: core::sync::atomic::AtomicUsize,
    #[cfg(picolv2_perf_diagnostics)]
    allocations: core::sync::atomic::AtomicUsize,
    #[cfg(picolv2_perf_diagnostics)]
    last_request: core::sync::atomic::AtomicUsize,
    #[cfg(picolv2_perf_diagnostics)]
    failures: core::sync::atomic::AtomicUsize,
    #[cfg(picolv2_perf_diagnostics)]
    failed_request: core::sync::atomic::AtomicUsize,
}

impl SystemHeap {
    pub(crate) const fn empty() -> Self {
        Self {
            heap: Heap::empty(),
            #[cfg(picolv2_perf_diagnostics)]
            current: core::sync::atomic::AtomicUsize::new(0),
            #[cfg(picolv2_perf_diagnostics)]
            peak: core::sync::atomic::AtomicUsize::new(0),
            #[cfg(picolv2_perf_diagnostics)]
            allocations: core::sync::atomic::AtomicUsize::new(0),
            #[cfg(picolv2_perf_diagnostics)]
            last_request: core::sync::atomic::AtomicUsize::new(0),
            #[cfg(picolv2_perf_diagnostics)]
            failures: core::sync::atomic::AtomicUsize::new(0),
            #[cfg(picolv2_perf_diagnostics)]
            failed_request: core::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(crate) unsafe fn init(&self, start: usize, size: usize) {
        unsafe { self.heap.init(start, size) };
    }

    pub(crate) fn log(&self, stage: &'static str) {
        #[cfg(picolv2_perf_diagnostics)]
        {
            use core::sync::atomic::Ordering;

            diag_info!(
                "heap {} current={} peak={} allocations={} last_request={} failures={} failed_request={}",
                stage,
                self.current.load(Ordering::Relaxed),
                self.peak.load(Ordering::Relaxed),
                self.allocations.load(Ordering::Relaxed),
                self.last_request.load(Ordering::Relaxed),
                self.failures.load(Ordering::Relaxed),
                self.failed_request.load(Ordering::Relaxed),
            );
        }
        #[cfg(not(picolv2_perf_diagnostics))]
        let _ = stage;
    }
}

unsafe impl GlobalAlloc for SystemHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(picolv2_perf_diagnostics)]
        self.last_request
            .store(layout.size(), core::sync::atomic::Ordering::Relaxed);
        let pointer = unsafe { self.heap.alloc(layout) };
        #[cfg(picolv2_perf_diagnostics)]
        if !pointer.is_null() {
            let current = self
                .current
                .fetch_add(layout.size(), core::sync::atomic::Ordering::Relaxed)
                + layout.size();
            self.allocations
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            self.peak
                .fetch_max(current, core::sync::atomic::Ordering::Relaxed);
        } else {
            self.failures
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            self.failed_request
                .store(layout.size(), core::sync::atomic::Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { self.heap.dealloc(pointer, layout) };
        #[cfg(picolv2_perf_diagnostics)]
        self.current
            .fetch_sub(layout.size(), core::sync::atomic::Ordering::Relaxed);
    }
}

pub(crate) struct Measurement {
    #[cfg(picolv2_perf_diagnostics)]
    start: embassy_time::Instant,
}

impl Measurement {
    #[inline(always)]
    fn start() -> Self {
        Self {
            #[cfg(picolv2_perf_diagnostics)]
            start: embassy_time::Instant::now(),
        }
    }

    #[cfg(picolv2_perf_diagnostics)]
    fn elapsed_micros(self) -> u64 {
        self.start.elapsed().as_micros()
    }
}

pub(crate) struct PluginPerformance {
    #[cfg(picolv2_perf_diagnostics)]
    node_micros: alloc::vec::Vec<u64>,
    #[cfg(picolv2_perf_diagnostics)]
    block_count: u32,
    #[cfg(picolv2_perf_diagnostics)]
    max_micros: u64,
    #[cfg(picolv2_perf_diagnostics)]
    overrun_count: u32,
}

impl PluginPerformance {
    pub(crate) fn new(node_count: usize) -> Self {
        #[cfg(not(picolv2_perf_diagnostics))]
        let _ = node_count;
        Self {
            #[cfg(picolv2_perf_diagnostics)]
            node_micros: alloc::vec![0; node_count],
            #[cfg(picolv2_perf_diagnostics)]
            block_count: 0,
            #[cfg(picolv2_perf_diagnostics)]
            max_micros: 0,
            #[cfg(picolv2_perf_diagnostics)]
            overrun_count: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn start() -> Measurement {
        Measurement::start()
    }

    #[inline(always)]
    pub(crate) fn node_finished(&mut self, node: usize, measurement: Measurement) {
        #[cfg(picolv2_perf_diagnostics)]
        {
            self.node_micros[node] += measurement.elapsed_micros();
        }
        #[cfg(not(picolv2_perf_diagnostics))]
        let _ = (node, measurement);
    }

    #[inline(always)]
    pub(crate) fn block_finished(&mut self, measurement: Measurement) {
        #[cfg(picolv2_perf_diagnostics)]
        {
            const BUDGET_MICROS: u64 = (BLOCK_SIZE as u64 * 1_000_000) / SAMPLE_RATE as u64;
            let elapsed_micros = measurement.elapsed_micros();
            self.max_micros = self.max_micros.max(elapsed_micros);
            if elapsed_micros > BUDGET_MICROS {
                self.overrun_count += 1;
            }
            self.block_count += 1;
            if self.block_count >= REPORT_BLOCKS {
                for (index, micros) in self.node_micros.iter_mut().enumerate() {
                    diag_info!(
                        "plugin node {} total={}us over {} blocks",
                        index,
                        *micros,
                        self.block_count
                    );
                    *micros = 0;
                }
                diag_info!(
                    "plugin process: budget={}us max={}us overruns={}/{}",
                    BUDGET_MICROS,
                    self.max_micros,
                    self.overrun_count,
                    self.block_count
                );
                self.block_count = 0;
                self.max_micros = 0;
                self.overrun_count = 0;
            }
        }
        #[cfg(not(picolv2_perf_diagnostics))]
        let _ = measurement;
    }
}

pub(crate) struct AudioOutputPerformance {
    #[cfg(picolv2_perf_diagnostics)]
    total_blocks: u32,
    #[cfg(picolv2_perf_diagnostics)]
    xrun_blocks: u32,
}

impl AudioOutputPerformance {
    pub(crate) const fn new() -> Self {
        Self {
            #[cfg(picolv2_perf_diagnostics)]
            total_blocks: 0,
            #[cfg(picolv2_perf_diagnostics)]
            xrun_blocks: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn block_finished(&mut self, rendered: bool) {
        #[cfg(picolv2_perf_diagnostics)]
        {
            self.total_blocks += 1;
            if !rendered {
                self.xrun_blocks += 1;
            }
            if self.total_blocks >= REPORT_BLOCKS {
                diag_info!(
                    "audio out: xruns={}/{} blocks",
                    self.xrun_blocks,
                    self.total_blocks
                );
                self.total_blocks = 0;
                self.xrun_blocks = 0;
            }
        }
        #[cfg(not(picolv2_perf_diagnostics))]
        let _ = rendered;
    }
}

pub(crate) fn plugin_log(bytes: &[u8]) {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    match core::str::from_utf8(bytes) {
        Ok(text) => diag_debug!("plugin: {}", text),
        Err(_) => diag_debug!("plugin: {=[u8]}", bytes),
    }
}

#[inline(always)]
pub(crate) fn midi_input(event_count: usize) {
    if event_count > 0 {
        diag_debug!("MIDI input block events={}", event_count);
    }
}

#[inline(always)]
pub(crate) fn atom_output(node: usize, port: u32, size: u32, header_size: u32) {
    if size > header_size {
        diag_debug!("MIDI output node={} port={} bytes={}", node, port, size);
    }
}
