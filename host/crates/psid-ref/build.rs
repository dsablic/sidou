// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..").canonicalize().unwrap();
    let player = root.join("third_party/fm1-sid-player");
    let chips = root.join("third_party/chips");
    let oracle = manifest.join("c/psid_oracle.c");
    for p in [&player, &chips, &oracle] {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    cc::Build::new()
        .file(&oracle)
        .include(&player)
        .include(&chips)
        .flag("-std=c11")
        .warnings(true)
        .extra_warnings(true)
        .compile("psid_oracle");
}
