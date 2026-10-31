//! Frame times, CPU use and memory of the running spike, in numbers that can be compared.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use cpu_time::ProcessTime;

use crate::bench::Mode;

/// Records the time between frames and, at the end, how busy the process was.
pub struct FrameRecorder {
    started: Instant,
    cpu_started: ProcessTime,
    last: Option<Instant>,
    intervals_ms: Vec<f32>,
    memory_start: Option<u64>,
    /// Taken after the warm-up: start-up (device, shaders, fonts) is not what a user waits on
    /// while the window is open, so the steady numbers leave it out.
    mark: Option<Mark>,
}

struct Mark {
    wall: Instant,
    cpu: ProcessTime,
    frames: usize,
}

impl FrameRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            cpu_started: ProcessTime::now(),
            last: None,
            intervals_ms: Vec::new(),
            memory_start: memory_bytes(),
            mark: None,
        }
    }

    /// Starts the steady-state window: what follows is measured without the start-up.
    pub fn mark(&mut self) {
        self.mark = Some(Mark {
            wall: Instant::now(),
            cpu: ProcessTime::now(),
            frames: self.frames(),
        });
    }

    /// Call once for every frame the toolkit draws.
    pub fn frame(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last {
            self.intervals_ms
                .push(now.duration_since(last).as_secs_f32() * 1000.0);
        }
        self.last = Some(now);
    }

    /// Frames recorded so far.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.intervals_ms.len() + usize::from(self.last.is_some())
    }

    /// One line of `key=value` pairs.
    #[must_use]
    pub fn report(&self, toolkit: &str, mode: &str) -> String {
        let wall = self.started.elapsed().as_secs_f64();
        let cpu = self.cpu_started.elapsed().as_secs_f64();
        let mut sorted = self.intervals_ms.clone();
        sorted.sort_by(f32::total_cmp);
        let at = |fraction: f32| -> f32 {
            if sorted.is_empty() {
                return 0.0;
            }
            let index = ((sorted.len() - 1) as f32 * fraction).round() as usize;
            sorted[index]
        };
        let mean = if sorted.is_empty() {
            0.0
        } else {
            sorted.iter().sum::<f32>() / sorted.len() as f32
        };
        let megabytes = |bytes: Option<u64>| bytes.map_or(f64::NAN, |b| b as f64 / 1_048_576.0);
        let steady = self.mark.as_ref().map_or_else(String::new, |mark| {
            let seconds = mark.wall.elapsed().as_secs_f64();
            let frames = self.frames() - mark.frames;
            format!(
                " steady seconds={seconds:.1} frames={frames} fps={:.1} cpu_percent={:.1}",
                frames as f64 / seconds,
                mark.cpu.elapsed().as_secs_f64() / seconds * 100.0,
            )
        });
        format!(
            "toolkit={toolkit} mode={mode} seconds={wall:.1} frames={} fps={:.1} \
             interval_ms mean={mean:.2} p50={:.2} p95={:.2} p99={:.2} max={:.2} \
             over_1.5x_median={} cpu_percent={:.1} memory_mb start={:.0} end={:.0}{steady}",
            self.frames(),
            self.frames() as f64 / wall,
            at(0.5),
            at(0.95),
            at(0.99),
            at(1.0),
            sorted.iter().filter(|ms| **ms > at(0.5) * 1.5).count(),
            cpu / wall * 100.0,
            megabytes(self.memory_start),
            megabytes(memory_bytes()),
        )
    }
}

impl Default for FrameRecorder {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL: OnceLock<Mutex<FrameRecorder>> = OnceLock::new();

/// The one recorder of the process: the draw code of either toolkit calls `frame()` on it. The
/// first call starts the clock.
pub fn global() -> &'static Mutex<FrameRecorder> {
    GLOBAL.get_or_init(|| Mutex::new(FrameRecorder::new()))
}

/// Records one drawn frame in the global recorder.
pub fn record_frame() {
    if let Ok(mut recorder) = global().lock() {
        recorder.frame();
    }
}

/// In a `--bench` run: after the chosen time, print the report and end the process. A thread of
/// its own, so it does not wake the interface and does not disturb the idle measurement.
pub fn exit_after_bench(toolkit: &'static str, mode: Mode) {
    global();
    let Some(duration) = mode.duration() else {
        return;
    };
    std::thread::spawn(move || {
        let warm_up = duration.min(std::time::Duration::from_secs(4));
        std::thread::sleep(warm_up);
        if let Ok(mut recorder) = global().lock() {
            recorder.mark();
        }
        std::thread::sleep(duration - warm_up);
        if let Ok(recorder) = global().lock() {
            println!("{}", recorder.report(toolkit, mode.label()));
        }
        std::process::exit(0);
    });
}

/// The process's physical memory in bytes, when the platform says.
#[must_use]
pub fn memory_bytes() -> Option<u64> {
    memory_stats::memory_stats().map(|stats| stats.physical_mem as u64)
}
