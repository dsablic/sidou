// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::env;
use std::path::PathBuf;

include!("../../build-support/sanitize.rs");

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let usb = manifest
        .join("../../../platform/fm1/bsp/chip")
        .canonicalize()
        .unwrap();
    let fm1 = manifest
        .join("../../../platform/fm1")
        .canonicalize()
        .unwrap();
    println!("cargo:rerun-if-changed=harness/ota_harness.c");
    println!("cargo:rerun-if-changed=harness/flash_hooks.c");
    println!("cargo:rerun-if-changed=harness/flash_spike_guard.c");
    println!(
        "cargo:rerun-if-changed={}",
        fm1.join("flash_spike_guard.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        fm1.join("ota_flash.h").display()
    );
    println!("cargo:rerun-if-changed={}", usb.join("ota.c").display());
    println!(
        "cargo:rerun-if-changed={}",
        fm1.join("loader/ldr_core.c").display()
    );
    let mut build = cc::Build::new();
    build
        .file(manifest.join("harness/ota_harness.c"))
        .file(manifest.join("harness/flash_hooks.c"))
        .file(manifest.join("harness/flash_spike_guard.c"))
        .include(&usb)
        .include(&fm1)
        .flag("-std=c11")
        .warnings(true)
        .flag_if_supported("-Wno-unused-function");
    sanitize::apply_sanitizer(&mut build);
    build.compile("ota_harness");
    uac_harnesses(&manifest, &fm1);
}

fn uac_harnesses(manifest: &std::path::Path, fm1: &std::path::Path) {
    println!("cargo:rerun-if-changed=harness/uac_harness.c");
    for f in [
        "bsp/chip/usb.c",
        "bsp/chip/usb_host.c",
        "bsp/chip/fm1_usb.h",
        "usb_config.h",
        "usb_audio.h",
        "usb_audio.c",
    ] {
        println!("cargo:rerun-if-changed={}", fm1.join(f).display());
    }
    let mut sidou = cc::Build::new();
    sidou
        .file(manifest.join("harness/uac_harness.c"))
        .include(fm1)
        .flag("-std=c11")
        .opt_level(2)
        .warnings(true)
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-unused-parameter");
    sanitize::apply_sanitizer(&mut sidou);
    sidou.compile("uac_harness");
    println!("cargo:rustc-link-lib=m");
}
