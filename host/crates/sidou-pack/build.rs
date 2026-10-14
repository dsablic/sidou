// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds the ota.c harness (harness/ota_harness.c): the firmware's step 1 of the update
//! protocol, for the tests.

use std::env;
use std::path::PathBuf;

include!("../../build-support/sanitize.rs");

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let usb = manifest
        .join("../../../platform/fm1/usb")
        .canonicalize()
        .unwrap();
    println!("cargo:rerun-if-changed=harness/ota_harness.c");
    println!("cargo:rerun-if-changed={}", usb.join("ota.c").display());
    let mut build = cc::Build::new();
    build
        .file(manifest.join("harness/ota_harness.c"))
        .include(&usb)
        .flag("-std=c11")
        .warnings(true)
        .flag_if_supported("-Wno-unused-function");
    sanitize::apply_sanitizer(&mut build);
    build.compile("ota_harness");
}
