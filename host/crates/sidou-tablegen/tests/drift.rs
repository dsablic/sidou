// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The checked-in tables must be exactly what sidou-tablegen produces now.

#[test]
fn checked_in_tables_match_the_generator() {
    let root = sidou_tablegen::repo_root();
    let mut stale = Vec::new();
    for g in sidou_tablegen::generate() {
        let path = root.join(g.path);
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        if on_disk != g.contents {
            stale.push(g.path);
        }
    }
    assert!(
        stale.is_empty(),
        "{} differ from the generator; from host/, run `cargo run --release -p sidou-tablegen` \
         and commit the result",
        stale.join(", ")
    );
}
