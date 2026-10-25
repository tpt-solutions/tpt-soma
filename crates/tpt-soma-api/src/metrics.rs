//! Minimal Prometheus text-format exposition (v0.0.4).
//!
//! In-house replacement for the Apache-2.0-only `prometheus` crate so the
//! dependency chain stays usable under plain MIT. Only monotonic counters are
//! supported; add gauges/histograms here if a caller actually needs them.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::{Mutex, OnceLock};

pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4";

fn counters() -> &'static Mutex<BTreeMap<String, u64>> {
    static COUNTERS: OnceLock<Mutex<BTreeMap<String, u64>>> = OnceLock::new();
    COUNTERS.get_or_init(Default::default)
}

/// Increment the named counter by `n`. `name` must be a valid Prometheus metric name.
pub fn inc_by(name: &str, n: u64) {
    let mut map = counters().lock().unwrap_or_else(|e| e.into_inner());
    *map.entry(name.to_string()).or_insert(0) += n;
}

pub fn inc(name: &str) {
    inc_by(name, 1);
}

/// Render all registered counters in Prometheus text exposition format.
pub fn render() -> String {
    let map = counters().lock().unwrap_or_else(|e| e.into_inner());
    let mut out = String::new();
    for (name, value) in map.iter() {
        let _ = writeln!(out, "# TYPE {name} counter");
        let _ = writeln!(out, "{name} {value}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_renders_in_exposition_format() {
        inc_by("tpt_soma_test_requests_total", 2);
        inc("tpt_soma_test_requests_total");
        let body = render();
        assert!(body.contains("# TYPE tpt_soma_test_requests_total counter\n"));
        assert!(body.contains("tpt_soma_test_requests_total 3\n"));
    }
}
