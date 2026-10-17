// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! RED MOON RITE's art: the PNGs in assets/scene/ are what assets/scene/red-moon-rite.js
//! draws, and the script draws the same PNGs on every run (pixels, palette and transparency:
//! Node's zlib may compress them to other bytes); so are its motion tables
//! (red-moon-rite-motion.txt). Needs Node; skipped without it.
//! `node assets/scene/red-moon-rite.js` rewrites them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sidou_tests::font::repo_root;

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A PNG's size, bit depth, palette, transparency and pixel rows, decoded.
type Decoded = (u32, u32, u8, Vec<u8>, Vec<u8>, Vec<u8>);

fn decode(name: &str, bytes: &[u8]) -> Decoded {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let info = dec.info();
    let head = (
        info.width,
        info.height,
        info.bit_depth as u8,
        info.palette
            .as_ref()
            .map(|p| p.to_vec())
            .unwrap_or_default(),
        info.trns.as_ref().map(|t| t.to_vec()).unwrap_or_default(),
    );
    let mut buf = vec![0; dec.output_buffer_size().expect("PNG size")];
    dec.next_frame(&mut buf)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    (head.0, head.1, head.2, head.3, head.4, buf)
}

/// Every PNG in `dir`, by name, decoded.
fn pngs(dir: &Path) -> BTreeMap<String, Decoded> {
    fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let decoded = decode(&name, &fs::read(&p).unwrap());
            (name, decoded)
        })
        .collect()
}

const MOTION: &str = "red-moon-rite-motion.txt";

/// Runs the script into a fresh directory and returns the PNGs it wrote, and its motion
/// tables.
fn export(run: &str) -> (BTreeMap<String, Decoded>, String) {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("red-moon-rite-{run}"));
    let _ = fs::remove_dir_all(&out);
    let script = repo_root().join("assets/scene/red-moon-rite.js");
    let status = Command::new("node")
        .arg(&script)
        .arg(&out)
        .status()
        .expect("node");
    assert!(status.success(), "{} failed: {status}", script.display());
    (
        pngs(&out),
        fs::read_to_string(out.join(MOTION)).expect(MOTION),
    )
}

#[test]
fn red_moon_rite_pngs_match_their_drawing_code() {
    if !node_available() {
        eprintln!("skipped: Node is not installed, so assets/scene/red-moon-rite.js cannot run");
        return;
    }
    let (first, motion) = export("a");
    let (second, motion2) = export("b");
    assert!(
        motion == motion2,
        "two runs of the script wrote different motion tables"
    );
    let committed_motion =
        fs::read_to_string(repo_root().join("assets/scene").join(MOTION)).unwrap();
    assert!(
        committed_motion == motion,
        "assets/scene/{MOTION} is behind its drawing code: run `node assets/scene/red-moon-rite.js`"
    );
    assert!(first.len() >= 35, "the script wrote {} PNGs", first.len());
    assert!(
        first == second,
        "two runs of the script drew different PNGs"
    );

    let committed = pngs(&repo_root().join("assets/scene"));
    let names = |m: &BTreeMap<String, Decoded>| m.keys().cloned().collect::<Vec<_>>();
    assert_eq!(
        names(&committed),
        names(&first),
        "assets/scene/ holds other PNGs than the script writes"
    );
    let stale: Vec<_> = first
        .iter()
        .filter(|(name, image)| committed[*name] != **image)
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(
        stale.is_empty(),
        "assets/scene/ is behind its drawing code ({stale:?}): run `node assets/scene/red-moon-rite.js`"
    );
}
