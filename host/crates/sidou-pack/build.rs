// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds the host harnesses (harness/): the firmware's step 1 of the update protocol
//! (ota.c) and its flash hooks (platform/fm1/ota_flash.h), for the tests.

use std::env;
use std::path::PathBuf;

include!("../../build-support/sanitize.rs");

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let usb = manifest
        .join("../../../platform/fm1/src")
        .canonicalize()
        .unwrap();
    let fm1 = manifest
        .join("../../../platform/fm1")
        .canonicalize()
        .unwrap();
    println!("cargo:rerun-if-changed=harness/ota_harness.c");
    println!("cargo:rerun-if-changed=harness/flash_hooks.c");
    println!(
        "cargo:rerun-if-changed={}",
        fm1.join("ota_flash.h").display()
    );
    println!("cargo:rerun-if-changed={}", usb.join("ota.c").display());
    let mut build = cc::Build::new();
    build
        .file(manifest.join("harness/ota_harness.c"))
        .file(manifest.join("harness/flash_hooks.c"))
        .include(&usb)
        .include(&fm1)
        .flag("-std=c11")
        .warnings(true)
        .flag_if_supported("-Wno-unused-function");
    sanitize::apply_sanitizer(&mut build);
    build.compile("ota_harness");
}
