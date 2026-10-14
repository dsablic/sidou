// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Compiles the C core and the workload with `cc` and generates their FFI with bindgen.
//! `wrap_static_fns` makes the header-only `static inline` functions callable: bindgen writes
//! an extern wrapper for each into a C file, which is compiled with the rest.

use std::env;
use std::path::{Path, PathBuf};

const SOURCES: [&str; 3] = [
    "core/sid/sid.c",
    "core/sid/sid_filter.c",
    "bench/workload.c",
];

/// SIDOU_SANITIZE=undefined traps on UB without a runtime library, so it works on macOS
/// and with stable Rust. SIDOU_SANITIZE=address needs the Rust side built with
/// -Zsanitizer=address (nightly), which provides the ASan runtime.
fn sanitize_flags() -> Vec<&'static str> {
    match env::var("SIDOU_SANITIZE").as_deref() {
        Err(_) | Ok("") => vec![],
        Ok("undefined") => vec!["-fsanitize=undefined", "-fsanitize-trap=undefined"],
        Ok("address") => vec!["-fsanitize=address", "-fno-omit-frame-pointer"],
        Ok(other) => panic!("SIDOU_SANITIZE={other}: expected undefined or address"),
    }
}

fn c_build(root: &Path) -> cc::Build {
    let mut build = cc::Build::new();
    build
        .include(root.join("core"))
        .include(root.join("bench"))
        .flag("-std=c11")
        // Float results must be identical on host and pi32v2 so their outputs can be compared.
        .flag("-ffp-contract=off")
        .warnings(true)
        .extra_warnings(true)
        .flag("-Wshadow")
        .warnings_into_errors(true);
    for flag in sanitize_flags() {
        build.flag(flag);
    }
    build
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..").canonicalize().unwrap();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wrapper = manifest.join("wrapper.h");

    println!("cargo:rerun-if-env-changed=SIDOU_SANITIZE");
    println!("cargo:rerun-if-changed={}", wrapper.display());
    println!("cargo:rerun-if-changed={}", root.join("core/sid").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bench/workload.h").display()
    );
    for s in SOURCES {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }

    let static_fns = out.join("static_fns");
    let bindings = bindgen::Builder::default()
        .header(wrapper.to_str().unwrap())
        .clang_arg(format!("-I{}", root.join("core").display()))
        .clang_arg(format!("-I{}", root.join("bench").display()))
        .clang_arg("-std=c11")
        .allowlist_function("sid_.*|sidou_.*|workload_.*")
        .allowlist_type("sid.*")
        .allowlist_var("SID_.*|SIDOU_.*|WORKLOAD_.*")
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
