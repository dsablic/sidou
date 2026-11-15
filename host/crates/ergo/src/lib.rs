// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod capture;
#[path = "../../emu-gate/tests/common/mod.rs"]
#[allow(
    clippy::all,
    unused_qualifications,
    unused_imports,
    reason = "emu-gate's test helpers, included by path, are linted with emu-gate"
)]
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

pub use common::par_map;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shard {
    index: usize,
    count: usize,
}

impl Shard {
    pub const ALL: Shard = Shard { index: 0, count: 1 };

    pub fn parse(text: &str) -> Result<Shard, String> {
        let bad = || format!("--shard takes K/N with 1 <= K <= N, not {text}");
        let (k, n) = text.split_once('/').ok_or_else(bad)?;
        let (k, n): (usize, usize) = (k.parse().map_err(|_| bad())?, n.parse().map_err(|_| bad())?);
        if k == 0 || k > n {
            return Err(bad());
        }
        Ok(Shard {
            index: k - 1,
            count: n,
        })
    }

    pub fn takes(self, i: usize) -> bool {
        i % self.count == self.index
    }
}

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
    use super::{par_map, slug, Shard};

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
    fn the_shards_of_a_split_take_every_item_once() {
        let shards: Vec<Shard> = (1..=3)
            .map(|k| Shard::parse(&format!("{k}/3")).unwrap())
            .collect();
        for i in 0..20 {
            assert_eq!(shards.iter().filter(|s| s.takes(i)).count(), 1, "item {i}");
        }
        assert!((0..20).all(|i| Shard::ALL.takes(i)));
        for bad in ["0/3", "4/3", "3", "a/b", "1/0"] {
            assert!(Shard::parse(bad).is_err(), "{bad}");
        }
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
