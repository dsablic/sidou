// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds the PSID oracle (M3 Task 7): `c/psid_oracle.c`, the test-only derived copy of FM-1
//! SID's `psid.c` (third_party/fm1-sid-player, BSD-2-Clause), with floooh/chips' `m6502.h`
//! (third_party/chips, zlib). This is the only build of either (LICENSING.md). Warnings on:
//! `psid.c` and `m6502.h` build clean with them. No sanitizer: this is the reference, not the
//! code under test.

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
