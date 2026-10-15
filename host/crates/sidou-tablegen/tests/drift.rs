// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The checked-in tables must be what sidou-tablegen produces now: the computed ones exactly
//! (every job), the measured filter tables up to the last printed digit (ignored: half a
//! minute of reSIDfp; CI runs it in one Linux job with `-- --ignored`).

const REGENERATE: &str =
    "from host/, run `cargo run --release -p sidou-tablegen` and commit the result";

fn on_disk(path: &str) -> String {
    std::fs::read_to_string(sidou_tablegen::repo_root().join(path)).unwrap_or_default()
}

#[test]
fn checked_in_tables_match_the_generator() {
    let stale: Vec<&str> = sidou_tablegen::generate_computed()
        .iter()
        .filter(|g| on_disk(g.path) != g.contents)
        .map(|g| g.path)
        .collect();
    assert!(
        stale.is_empty(),
        "{} differ from the generator; {REGENERATE}",
        stale.join(", ")
    );
}

/// The filter tables re-measured from reSIDfp. A last-digit difference is a platform's libm
/// (reSIDfp's filter model and resampler use it), not a stale table: keep the checked-in
/// file.
#[test]
#[ignore = "half a minute of reSIDfp measurements; CI runs it in one Linux job"]
fn checked_in_filter_tables_match_the_measurement() {
    let g = sidou_tablegen::filter_header(&sidou_tablegen::filter_fits());
    if let Some(d) = sidou_tablegen::differs_beyond_last_digit(&on_disk(g.path), &g.contents) {
        panic!(
            "{} differs from the measurement ({d}); {REGENERATE}",
            g.path
        );
    }
}
