// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds GoatTracker 2.77's song, table and relocation code (third_party/goattracker2,
//! unmodified) with SIDOU's shim (shim/: BME's calls, files in memory, the assembler) into
//! a static library. GoatTracker's files are compiled with `fopen`, `fread` and `exit` renamed to
//! the shim's and gt2reloc's `main` renamed out of the way (`shim/gt2ref_rename.h`); their warnings are GoatTracker's and are
//! not ours to fix, so they are silenced. No sanitizer: this is the reference, not the code
//! under test.

use std::path::PathBuf;

const GOATTRACKER: [&str; 7] = [
    "gt2reloc.c",
    "gsong.c",
    "gtable.c",
    "ginstr.c",
    "gorder.c",
    "gpattern.c",
    "gplay.c",
];
const SHIM: [&str; 3] = ["gt2ref_shim.c", "gt2ref_asm.c", "gt2ref.c"];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..").canonicalize().unwrap();
    let gt = root.join("third_party/goattracker2/src");
    let shim = manifest.join("shim");
    println!("cargo:rerun-if-changed={}", gt.display());
    println!("cargo:rerun-if-changed={}", shim.display());

    let mut build = cc::Build::new();
    build
        .include(&shim)
        .include(&gt)
        // fopen, fread, exit and main renamed after the C library's headers
        // (gt2ref_rename.h says why): files in memory, a short read of the song recorded
        // (loadsong itself reports nothing), and an exit on an assembly error that returns
        // to the caller.
        .flag("-include")
        .flag(shim.join("gt2ref_rename.h").to_str().unwrap())
        .warnings(false)
        .flag_if_supported("-w")
        .flag_if_supported("-fno-strict-aliasing")
        // In .bss, not as common symbols, which macOS's linker warns about at their alignment.
        .flag_if_supported("-fno-common");
    for f in GOATTRACKER {
        build.file(gt.join(f));
    }
    // One library for both halves: they call each other, which a one-pass linker resolves
    // only within an archive.
    let objects = build.compile_intermediates();

    let mut build = cc::Build::new();
    build
        .include(&shim)
        .include(&gt)
        .flag("-std=gnu11")
        .define(
            "GT2REF_SRC_DIR",
            Some(format!("\"{}\"", gt.display()).as_str()),
        )
        .warnings(true)
        .extra_warnings(true)
        // GoatTracker's headers, which gt2ref.c includes, are not warning-clean.
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-sign-compare");
    for f in SHIM {
        build.file(shim.join(f));
    }
    build.objects(objects);
    build.compile("gt2ref");
    println!("cargo:rustc-link-lib=m");
}
