//! In-memory performance monitoring system.
//!
//! Provides low-overhead profiling with real-time stats collection.
//! Toggle with F12.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

/// Number of recent samples to keep for trend analysis
const SAMPLE_RING_SIZE: usize = 64;

/// Frame budget for 60fps (milliseconds)
pub const FRAME_BUDGET_60FPS: f64 = 16.67;

/// Frame budget for 30fps (milliseconds)
pub const FRAME_BUDGET_30FPS: f64 = 33.33;

/// Ring buffer for recent samples
pub struct RingBuffer<T: Copy + Default> {
    data: Vec<T>,
    write_pos: usize,
    len: usize,
}

impl<T: Copy + Default> Default for RingBuffer<T> {
    fn default() -> Self {
        Self {
            data: vec![T::default(); SAMPLE_RING_SIZE],
            write_pos: 0,
            len: 0,
        }
    }
}

impl<T: Copy + Default + Ord> RingBuffer<T> {
    pub fn push(&mut self, value: T) {
        self.data[self.write_pos] = value;
        self.write_pos = (self.write_pos + 1) % SAMPLE_RING_SIZE;
        if self.len < SAMPLE_RING_SIZE {
            self.len += 1;
        }
    }

    pub fn recent(&self, count: usize) -> Vec<T> {
        if self.len == 0 {
            return Vec::new();
        }
        let count = count.min(self.len);
        let mut result = Vec::with_capacity(count);
        let start = if self.len < SAMPLE_RING_SIZE {
            0
        } else {
            self.write_pos
        };
        for i in 0..count {
            let idx = (start + self.len - count + i) % SAMPLE_RING_SIZE;
            result.push(self.data[idx]);
        }
        result
    }
}

/// Single operation's accumulated statistics
pub struct OpStats {
    /// Total invocation count
    pub count: AtomicU64,
    /// Total time in microseconds
    pub total_us: AtomicU64,
    /// Maximum single execution time in microseconds
    pub max_us: AtomicU64,
    /// Recent samples ring buffer (microseconds)
    pub samples: RwLock<RingBuffer<u64>>,
}

impl Default for OpStats {
    fn default() -> Self {
        Self {
            count: AtomicU64::new(0),
            total_us: AtomicU64::new(0),
            max_us: AtomicU64::new(0),
            samples: RwLock::new(RingBuffer::default()),
        }
    }
}

/// Global performance metrics collector
pub struct PerfMetrics {
    /// Whether performance monitoring is enabled
    pub enabled: AtomicBool,
    /// Per-operation statistics
    ops: RwLock<HashMap<&'static str, OpStats>>,
    /// Frame time ring buffer (microseconds)
    frame_times: RwLock<RingBuffer<u64>>,
    /// Frame start time
    frame_start: RwLock<Option<Instant>>,
    /// Total frames counted
    pub frame_count: AtomicU64,
    /// Last CPU measurement time and ticks
    last_cpu_measure: RwLock<(Instant, u64)>,
    /// Last stats refresh time
    last_stats_refresh: RwLock<Instant>,
    /// CPU usage percentage (0-100)
    cpu_usage: RwLock<f32>,
    /// Memory usage in bytes
    memory_bytes: RwLock<u64>,
}

impl Default for PerfMetrics {
    fn default() -> Self {
        let (cpu_ticks, mem_bytes) = read_proc_stat().unwrap_or((0, 0));

        Self {
            enabled: AtomicBool::new(false),
            ops: RwLock::new(HashMap::new()),
            frame_times: RwLock::new(RingBuffer::default()),
            frame_start: RwLock::new(None),
            frame_count: AtomicU64::new(0),
            last_cpu_measure: RwLock::new((Instant::now(), cpu_ticks)),
            last_stats_refresh: RwLock::new(Instant::now()),
            cpu_usage: RwLock::new(0.0),
            memory_bytes: RwLock::new(mem_bytes),
        }
    }
}

/// Read CPU ticks and memory from /proc/self/stat and /proc/self/statm
fn read_proc_stat() -> Option<(u64, u64)> {
    // Read CPU ticks from /proc/self/stat
    // Format: pid (comm) state ... utime stime ...
    // Fields 14 and 15 (0-indexed: 13, 14) are utime and stime
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let parts: Vec<&str> = stat.split_whitespace().collect();
    if parts.len() < 15 {
        return None;
    }
    let utime: u64 = parts[13].parse().ok()?;
    let stime: u64 = parts[14].parse().ok()?;
    let cpu_ticks = utime + stime;
    
    // Read memory from /proc/self/statm (in pages)
    // First field is total program size, second is RSS
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let mem_parts: Vec<&str> = statm.split_whitespace().collect();
    let rss_pages: u64 = mem_parts.get(1)?.parse().ok()?;
    let page_size = 4096u64; // Standard page size
    let mem_bytes = rss_pages * page_size;
    
    Some((cpu_ticks, mem_bytes))
}

lazy_static::lazy_static! {
    pub static ref PERF: PerfMetrics = PerfMetrics::default();
}

impl PerfMetrics {
    /// Record operation timing
    pub fn record_op(&self, name: &'static str, duration_us: u64) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let mut ops = self.ops.write().unwrap();
        let stats = ops.entry(name).or_default();
        stats.count.fetch_add(1, Ordering::Relaxed);
        stats.total_us.fetch_add(duration_us, Ordering::Relaxed);
        stats.max_us.fetch_max(duration_us, Ordering::Relaxed);
        if let Ok(mut samples) = stats.samples.write() {
            samples.push(duration_us);
        }
    }

    /// Start a new frame
    pub fn frame_start(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        *self.frame_start.write().unwrap() = Some(Instant::now());
    }

    /// End frame and record frame time
    pub fn frame_end(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        if let Some(start) = self.frame_start.read().unwrap().as_ref() {
            let frame_time = start.elapsed().as_micros() as u64;
            self.frame_times.write().unwrap().push(frame_time);
            self.frame_count.fetch_add(1, Ordering::Relaxed);
        }

        // Check if stats need refresh (time-based, not frame-based)
        use crate::constants::PERF_STATS_REFRESH_MS;
        let last_refresh = *self.last_stats_refresh.read().unwrap();
        if last_refresh.elapsed().as_millis() >= PERF_STATS_REFRESH_MS as u128 {
            self.refresh_system_stats();
            *self.last_stats_refresh.write().unwrap() = Instant::now();
        }
    }

    /// Refresh CPU and memory stats
    fn refresh_system_stats(&self) {
        if let Some((cpu_ticks, mem_bytes)) = read_proc_stat() {
            let mut last = self.last_cpu_measure.write().unwrap();
            let now = Instant::now();
            let elapsed = now.duration_since(last.0).as_secs_f32();
            
            if elapsed > 0.0 {
                let tick_delta = cpu_ticks.saturating_sub(last.1);
                // Convert ticks to seconds (usually 100 ticks/sec on Linux)
                let cpu_seconds = tick_delta as f32 / 100.0;
                // CPU percentage = (cpu_time / wall_time) * 100
                let cpu_pct = (cpu_seconds / elapsed) * 100.0;
                *self.cpu_usage.write().unwrap() = cpu_pct;
            }
            
            *last = (now, cpu_ticks);
            *self.memory_bytes.write().unwrap() = mem_bytes;
        }
    }

    /// Get snapshot of metrics for display
    pub fn snapshot(&self) -> PerfSnapshot {
        let ops = self.ops.read().unwrap();
        let frame_times = self.frame_times.read().unwrap();

        let mut op_snapshots: Vec<OpSnapshot> = ops
            .iter()
            .map(|(name, stats)| {
                let samples = stats.samples.read().unwrap();
                let recent = samples.recent(SAMPLE_RING_SIZE);
                let count = recent.len();

                // Calculate mean
                let mean_us = if count > 0 {
                    recent.iter().sum::<u64>() as f64 / count as f64
                } else {
                    0.0
                };

                // Calculate standard deviation
                let std_us = if count > 1 {
                    let variance = recent.iter()
                        .map(|&x| {
                            let diff = x as f64 - mean_us;
                            diff * diff
                        })
                        .sum::<f64>() / (count - 1) as f64;
                    variance.sqrt()
                } else {
                    0.0
                };

                OpSnapshot {
                    name,
                    total_ms: stats.total_us.load(Ordering::Relaxed) as f64 / 1000.0,
                    mean_ms: mean_us / 1000.0,
                    std_ms: std_us / 1000.0,
                }
            })
            .collect();

        // Sort by total time descending (hotspots first)
        op_snapshots.sort_by(|a, b| b.total_ms.partial_cmp(&a.total_ms).unwrap_or(std::cmp::Ordering::Equal));

        let frame_samples: Vec<f64> = frame_times
            .recent(40)
            .iter()
            .map(|&us| us as f64 / 1000.0)
            .collect();

        let frame_avg_ms = if frame_samples.is_empty() {
            0.0
        } else {
            frame_samples.iter().sum::<f64>() / frame_samples.len() as f64
        };

        PerfSnapshot {
            ops: op_snapshots,
            frame_times_ms: frame_samples.clone(),
            frame_avg_ms,
            frame_max_ms: frame_samples.iter().cloned().fold(0.0, f64::max),
            cpu_usage: *self.cpu_usage.read().unwrap(),
            memory_mb: *self.memory_bytes.read().unwrap() as f64 / (1024.0 * 1024.0),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        *self.ops.write().unwrap() = HashMap::new();
        *self.frame_times.write().unwrap() = RingBuffer::default();
        self.frame_count.store(0, Ordering::Relaxed);
    }

    /// Toggle monitoring on/off, returns new state
    pub fn toggle(&self) -> bool {
        let new_state = !self.enabled.load(Ordering::Relaxed);
        self.enabled.store(new_state, Ordering::Relaxed);
        if new_state {
            self.reset();
            // Do initial system stats refresh when enabling
            self.refresh_system_stats();
        }
        new_state
    }
}

/// Snapshot of operation statistics for display
#[derive(Clone)]
pub struct OpSnapshot {
    pub name: &'static str,
    pub total_ms: f64,
    pub mean_ms: f64,
    pub std_ms: f64,
}

/// Snapshot of all metrics for display
#[derive(Clone)]
pub struct PerfSnapshot {
    pub ops: Vec<OpSnapshot>,
    pub frame_times_ms: Vec<f64>,
    pub frame_avg_ms: f64,
    pub frame_max_ms: f64,
    pub cpu_usage: f32,
    pub memory_mb: f64,
}
