// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds reSIDfp from the third_party/libresidfp submodule, plus our extern "C" shim.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const SOURCES: [&str; 18] = [
    "Dac.cpp",
    "EnvelopeGenerator.cpp",
    "ExternalFilter.cpp",
    "Filter.cpp",
    "Filter6581.cpp",
    "Filter8580.cpp",
    "FilterModelConfig.cpp",
    "FilterModelConfig6581.cpp",
    "FilterModelConfig8580.cpp",
    "Integrator6581.cpp",
    "Integrator8580.cpp",
    "OpAmp.cpp",
    "SID.cpp",
    "Spline.cpp",
    "State.cpp",
    "WaveformCalculator.cpp",
    "WaveformGenerator.cpp",
    "resample/SincResampler.cpp",
];

/// The values the CMake build passed to configure_file.
const CONFIG: [(&str, &str); 7] = [
    ("RESIDFP_INLINING", "1"),
    ("RESIDFP_INLINE", "inline"),
    ("RESIDFP_BRANCH_HINTS", "1"),
    ("HAVE_BUILTIN_EXPECT", "1"),
    ("PACKAGE_VERSION", "git"),
    ("LIB_MAJOR", "3"),
    ("LIB_MINOR", "0"),
];
const LIB_LEVEL: (&str, &str) = ("LIB_LEVEL", "0");

/// The first `@NAME@` (upper case, digits, underscores) still in `text`, as configure_file
/// with @ONLY would substitute it.
fn leftover_placeholder(text: &str) -> Option<&str> {
    let parts: Vec<&str> = text.split('@').collect();
    let between = parts.get(1..parts.len().saturating_sub(1)).unwrap_or(&[]);
    between.iter().copied().find(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    })
}

fn configure(template: &Path, out: &Path) {
    let mut text =
        fs::read_to_string(template).unwrap_or_else(|e| panic!("read {}: {e}", template.display()));
    for (key, value) in CONFIG.iter().chain([&LIB_LEVEL]) {
        text = text.replace(&format!("@{key}@"), value);
    }
    if let Some(var) = leftover_placeholder(&text) {
        panic!("{}: no value for @{var}@", template.display());
    }
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    fs::write(out, text).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest
        .join("../../../third_party/libresidfp/src")
        .canonicalize()
        .ok()
        .filter(|p| p.join("SID.cpp").exists())
        .expect("third_party/libresidfp is missing; run: git submodule update --init");
    let gen = PathBuf::from(env::var("OUT_DIR").unwrap()).join("residfp-gen");

    println!("cargo:rerun-if-env-changed=SIDOU_SANITIZE");
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("shim.cpp").display()
    );
    println!("cargo:rerun-if-changed={}", src.display());

    configure(&src.join("siddefs-fp.h.in"), &gen.join("siddefs-fp.h"));
    configure(
        &src.join("residfp/sidversion.h.in"),
        &gen.join("residfp/sidversion.h"),
    );

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(&src)
        .include(&gen)
        // Upstream builds with -ffast-math -fno-unsafe-math-optimizations. We deliberately
        // don't: the reference should follow strict IEEE semantics like our own core.
        .warnings(false)
        .flag("-w")
        .files(SOURCES.iter().map(|s| src.join(s)))
        .file(manifest.join("shim.cpp"));
    match env::var("SIDOU_SANITIZE").as_deref() {
        Ok("undefined") => {
            build
                .flag("-fsanitize=undefined")
                .flag("-fsanitize-trap=undefined");
        }
        Ok("address") => {
            build
                .flag("-fsanitize=address")
                .flag("-fno-omit-frame-pointer");
        }
        _ => {}
    }
    build.compile("residfp");
}
