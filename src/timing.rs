//! Where a hop spends its time: `mukade hop … --timing` prints one line per phase on stderr.
//!
//! Off by default, and then free: a phase is a [`span`] that records its duration when dropped, and
//! does nothing unless [`enable`] was called.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static ON: AtomicBool = AtomicBool::new(false);
static PHASES: Mutex<Vec<(&'static str, Duration)>> = Mutex::new(Vec::new());

/// Start recording phases.
pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

/// A running phase: its duration is recorded under `name` when it is dropped.
pub struct Span {
    name: &'static str,
    started: Option<Instant>,
}

pub fn span(name: &'static str) -> Span {
    Span {
        name,
        started: ON.load(Ordering::Relaxed).then(Instant::now),
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            PHASES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((self.name, started.elapsed()));
        }
    }
}

/// The phases recorded so far, as the table `--timing` prints: a phase that ran more than once is
/// summed, in the order each first ran.
pub fn report() -> String {
    let phases = PHASES.lock().unwrap_or_else(|e| e.into_inner());
    let mut rows: Vec<(&'static str, Duration)> = Vec::new();
    for (name, took) in phases.iter() {
        match rows.iter_mut().find(|(n, _)| n == name) {
            Some((_, total)) => *total += *took,
            None => rows.push((name, *took)),
        }
    }
    let mut out = String::new();
    for (name, took) in rows {
        out.push_str(&format!(
            "{name:<28} {:>9.1} ms\n",
            took.as_secs_f64() * 1000.0
        ));
    }
    out
}
