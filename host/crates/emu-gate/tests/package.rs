// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End to end: the update package (`sidou.fwsc`, from `sidou-pack`) booted as fm1-emu boots a
//! package (`stock_from_package` + `stock_boot`): the app comes from the package's scrambled
//! app area, the flash chip holds its flash.bin. The live instrument plays, sample for sample
//! as the bare image does.
//!
//! The package is `$SIDOU_FWSC`, else `sidou.fwsc` in the fm1 build directory (CI:
//! `cargo run -p sidou-pack -- ... -o build/fm1/sidou.fwsc`); without it the test skips.
//! It writes `sidou_package.json` and the audio, `sidou_package.wav` (A4 held from power-on),
//! into the fm1 build directory. A package older than the build's `sidou_app.bin` fails the
//! test: it holds an earlier build, so it would be compared with an image it was not made from.

mod common;

use std::path::{Path, PathBuf};

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

/// Why `package` cannot stand for `image`: it was written before the image was, so it holds an
/// earlier build. None when it is at least as new.
fn stale(package: &Path, image: &Path) -> Option<String> {
    let modified = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    };
    (modified(package) < modified(image)).then(|| {
        format!(
            "{} is older than {}: it holds an earlier build. Run sidou-pack again (TESTING.md, \
             Package tests)",
            package.display(),
            image.display()
        )
    })
}

#[test]
fn a_package_older_than_the_image_is_stale() {
    let dir = std::env::temp_dir().join(format!("sidou-package-stale-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (package, image) = (dir.join("sidou.fwsc"), dir.join("sidou_app.bin"));
    let at = |p: &Path, secs: u64| {
        let f = std::fs::File::create(p).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    };
    at(&image, 2_000_000);
    at(&package, 1_000_000);
    let why = stale(&package, &image).expect("an older package is stale");
    assert!(why.contains("sidou.fwsc is older than") && why.contains("sidou-pack"));
    at(&package, 2_000_000);
    assert_eq!(stale(&package, &image), None, "as new is fresh");
    at(&package, 3_000_000);
    assert_eq!(stale(&package, &image), None);
    std::fs::remove_dir_all(&dir).unwrap();
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
    if let Some(why) = stale(&path, &build.join("platform/fm1/sidou_app.bin")) {
        panic!("{why}");
    }
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
    let report = serde_json::json!({
        "package": path.display().to_string(),
        "package_bytes": pkg.len(),
        "frames": a.len(),
        "f0_hz": f0,
        "peak": peak(&tail),
        "first_difference_from_bare_image": first_diff,
    });
    // Something to listen to: the package boot's left channel (it is mono) as a WAV.
    let wav = build.join("sidou_package.wav");
    std::fs::write(&wav, wav_mono(&a, 44_118))
        .unwrap_or_else(|e| panic!("write {}: {e}", wav.display()));
    let out = build.join("sidou_package.json");
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));
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

/// A 16-bit mono PCM WAV file.
fn wav_mono(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}
