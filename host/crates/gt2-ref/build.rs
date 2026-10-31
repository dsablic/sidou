// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
        .flag("-include")
        .flag(shim.join("gt2ref_rename.h").to_str().unwrap())
        .warnings(false)
        .flag_if_supported("-w")
        .flag_if_supported("-fno-strict-aliasing")
        .flag_if_supported("-fno-common");
    for f in GOATTRACKER {
        build.file(gt.join(f));
    }
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
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-sign-compare");
    for f in SHIM {
        build.file(shim.join(f));
    }
    build.objects(objects);
    build.compile("gt2ref");
    println!("cargo:rustc-link-lib=m");
}
