// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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

#[test]
fn checked_in_filter_tables_match_the_measurement() {
    let g = sidou_tablegen::filter_header(&sidou_tablegen::filter_fits());
    if let Some(d) = sidou_tablegen::differs_beyond_last_digit(&on_disk(g.path), &g.contents) {
        panic!(
            "{} differs from the measurement ({d}); {REGENERATE}",
            g.path
        );
    }
}
