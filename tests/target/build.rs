// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = [
        "core/sid/sid.c",
        "core/sid/sid_filter.c",
        "bench/workload.c",
    ];
    for s in &sources {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }
    println!("cargo:rerun-if-changed={}", root.join("core/sid").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bench/workload.h").display()
    );
    cc::Build::new()
        .files(sources.iter().map(|s| root.join(s)))
        .include(root.join("core"))
        .include(root.join("bench"))
        .flag("-std=c11")
        .flag("-ffp-contract=off")
        .opt_level(2)
        .compile("sidou_host");
}
