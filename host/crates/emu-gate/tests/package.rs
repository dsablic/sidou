// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the update package (`sidou.fwsc`, from `sidou-pack`) booted as fm1-emu boots a
//! package (`stock_from_package` + `stock_boot`): the app comes from the package's scrambled
//! app area, the flash chip holds its flash.bin. The live instrument plays, sample for sample
//! as the bare image does.
//!
//! The package is `$SIDOU_FWSC`, else `sidou.fwsc` in the fm1 build directory (CI:
//! `cargo run -p sidou-pack -- ... -o build/fm1/sidou.fwsc`); without it the test skips.

mod common;

use std::path::PathBuf;

use common::{build_dir, peak, pitch, Firmware, Machine};

const RUN_MS: u64 = 900;
/// The A4 key (Felucca id 30), held from power-on.
const A4_KEY: u8 = 30;

fn package_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SIDOU_FWSC").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    Some(build_dir()?.join("sidou.fwsc"))
}

#[test]
fn the_package_boots_and_plays_as_the_bare_image() {
    let (Some(build), Some(path)) = (build_dir(), package_path()) else {
        common::skip_message("the package boot test");
        return;
    };
    let Ok(pkg) = std::fs::read(&path) else {
        println!(
            "skipping the package boot test: no {} (run sidou-pack first)",
            path.display()
        );
        return;
    };
    let fw = Firmware::load(&build);
    let mut from_pkg = Machine::boot_package(&pkg);
    from_pkg.key(A4_KEY, true);
    from_pkg.run_ms(RUN_MS);
    let mut bare = Machine::boot(&fw.image);
    bare.key(A4_KEY, true);
    bare.run_ms(RUN_MS);

    let (a, b) = (from_pkg.left(), bare.left());
    let tail: Vec<f32> = a[a.len() / 2..].iter().map(|&s| f32::from(s)).collect();
    let f0 = pitch(&tail);
    let first_diff = a.iter().zip(&b).position(|(x, y)| x != y);
    println!(
        "{}: {} bytes; {} frames from the package, {} from the bare image, first difference \
         {first_diff:?}; A4 {f0:.2} Hz, peak {}",
        path.display(),
        pkg.len(),
        a.len(),
        b.len(),
        peak(&tail)
    );
    assert!(
        a.len() > 15_000,
        "the package boot played {} frames",
        a.len()
    );
    assert!(peak(&tail) > 2000.0, "the package boot is silent");
    assert!(
        (f0 / 440.0 - 1.0).abs() < 0.005,
        "the package boot plays {f0:.2} Hz"
    );
    assert_eq!(a.len(), b.len(), "the package boot played another length");
    assert_eq!(
        first_diff, None,
        "the package boot differs from the bare image"
    );
}
