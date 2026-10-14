// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Compiles the C core, the workload and the portable app logic (`app/`) with `cc` and generates their FFI with bindgen.
//! `wrap_static_fns` makes the header-only `static inline` functions callable: bindgen writes
//! an extern wrapper for each into a C file, which is compiled with the rest.

use std::env;
use std::path::{Path, PathBuf};

const SOURCES: [&str; 11] = [
    "core/sid/sid.c",
    "core/sid/sid_filter.c",
    "bench/workload.c",
    "app/live.c",
    "app/live_freq.c",
    "app/midi_in.c",
    "app/midi_ring.c",
    "app/midi_rx.c",
    "app/midi_usb.c",
    "app/panel.c",
    "app/reg_ring.c",
];

/// Include roots, relative to the repository: core headers are "sid/...", the others bare.
const INCLUDES: [&str; 3] = ["core", "bench", "app"];

include!("../../build-support/sanitize.rs");

fn c_build(root: &Path) -> cc::Build {
    let mut build = cc::Build::new();
    for dir in INCLUDES {
        build.include(root.join(dir));
    }
    build
        .flag("-std=c11")
        // Float results must be identical on host and pi32v2 so their outputs can be compared.
        .flag("-ffp-contract=off")
        .warnings(true)
        .extra_warnings(true)
        .flag("-Wshadow")
        .warnings_into_errors(true);
    sanitize::apply_sanitizer(&mut build);
    build
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..").canonicalize().unwrap();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wrapper = manifest.join("wrapper.h");

    println!("cargo:rerun-if-changed={}", wrapper.display());
    println!("cargo:rerun-if-changed={}", root.join("core/sid").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bench/workload.h").display()
    );
    println!("cargo:rerun-if-changed={}", root.join("app").display());
    for s in SOURCES {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }

    let static_fns = out.join("static_fns");
    let bindings = bindgen::Builder::default()
        .header(wrapper.to_str().unwrap())
        .clang_args(INCLUDES.map(|dir| format!("-I{}", root.join(dir).display())))
        .clang_arg("-std=c11")
        .allowlist_function("sid_.*|sidou_.*|workload_.*|live_.*|midi_.*|panel_.*|reg_ring_.*")
        .allowlist_type("sid.*|live.*|midi_.*|panel.*|reg_ring.*")
        .allowlist_var("SID_.*|SIDOU_.*|WORKLOAD_.*|LIVE_.*|MIDI_.*|PANEL_.*|REG_RING_.*|live_.*")
        // A `static const` table private to sid_env.h; it has no symbol to link to.
        .blocklist_item("SID_RATE_PERIOD")
        .prepend_enum_name(false)
        .derive_default(true)
        .derive_debug(true)
        .wrap_static_fns(true)
        .wrap_static_fns_path(&static_fns)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen failed; it needs libclang: install clang or Xcode, or set LIBCLANG_PATH");
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("write bindings.rs");

    c_build(&root)
        .files(SOURCES.iter().map(|s| root.join(s)))
        .file(static_fns.with_extension("c"))
        .include(&manifest)
        .compile("sidou");
}
