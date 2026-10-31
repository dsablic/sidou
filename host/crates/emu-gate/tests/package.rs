// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod common;

use std::path::{Path, PathBuf};

use common::{build_dir, peak, pitch, Firmware, Machine};

const RUN_MS: u64 = 900;
const A4_KEY: u8 = common::panel::KEY0 + 16;

fn package_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SIDOU_FWSC").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    Some(build_dir()?.join("sidou.fwsc"))
}

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
    let wav = build.join("sidou_package.wav");
    std::fs::write(&wav, sidou_tests::showcase::wav(&a, common::SAMPLE_RATE))
        .unwrap_or_else(|e| panic!("write {}: {e}", wav.display()));
    let out = build.join("sidou_package.json");
    std::fs::write(&out, report.to_string())
        .unwrap_or_else(|e| panic!("write report {}: {e}", out.display()));
    assert!(
        a.len() > 15_000,
        "the package boot played {} frames",
        a.len()
    );
    assert!(
        peak(&tail) > 2000.0 * common::boot_gain(),
        "the package boot is silent"
    );
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
