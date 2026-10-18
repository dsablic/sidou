// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The ergonomics harness: drives the firmware in fm1-emu, scores it against explicit ergonomic
//! rules and writes a report (docs/TESTING.md, "The ergonomics harness").

pub mod capture;
/// emu-gate's emulator helpers, included by path rather than copied.
#[path = "../../emu-gate/tests/common/mod.rs"]
#[allow(clippy::all, unused_qualifications, unused_imports)]
pub mod common;
pub mod config;
pub mod device;
pub mod feedback;
pub mod findings;
pub mod journeys;
pub mod knobs;
pub mod metrics;
pub mod readability;
pub mod screen;

/// `f` over `items` on every core, results in the items' order (emulator runs are independent,
/// each in its own `Machine`; nothing here depends on the order they finish in).
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    let threads = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .min(items.len().max(1));
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner()
        .unwrap()
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

/// `K1 (CHIP)` -> `k1-chip`: a stable id part.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '+' {
            out.push_str("plus");
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::{par_map, slug};

    #[test]
    fn par_map_keeps_the_order() {
        let v: Vec<u32> = (0..50).collect();
        assert_eq!(
            par_map(&v, |x| x * 2),
            (0..50).map(|x| x * 2).collect::<Vec<_>>()
        );
        assert!(par_map(&Vec::<u32>::new(), |x| *x).is_empty());
    }

    #[test]
    fn slugs_are_stable_ids() {
        assert_eq!(slug("K1 (CHIP)"), "k1-chip");
        assert_eq!(slug("OCT+"), "octplus");
        assert_eq!(slug("get back to LIVE"), "get-back-to-live");
    }
}
pub mod consistency;
pub mod defaults;
pub mod recipe;
pub mod report;
pub mod run;
pub mod tasks;
