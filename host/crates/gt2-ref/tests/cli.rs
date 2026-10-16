// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `gt2-trace`'s command line: every flag strict, a mistake an error rather than a guess.

use std::process::Command;

fn gt2_trace(args: &[&str]) -> (bool, String) {
    let song = gt2_ref::examples_dir().join("consultant.sng");
    let out = Command::new(env!("CARGO_BIN_EXE_gt2-trace"))
        .args(
            args.iter()
                .map(|a| a.replace("SONG", song.to_str().unwrap())),
        )
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

#[test]
fn good_command_lines_trace() {
    for args in [
        vec!["SONG", "--frames", "2"],
        vec![
            "SONG",
            "--subtune",
            "0",
            "--frames",
            "2",
            "-S2",
            "-B1",
            "-AF000",
        ],
        vec![
            "SONG", "--frames", "2", "-N", "-P", "-W20", "-ZE0", "-C1", "-G432.5", "-I0",
        ],
        vec![
            "--frames", "2", "-O0", "-R0", "-V0", "-LD400", "-J1", "-D1", "-E1", "-H1", "SONG",
        ],
    ] {
        let (ok, out) = gt2_trace(&args);
        assert!(ok, "{args:?}: {out}");
        assert!(out.starts_with("init:"), "{args:?}: {out}");
    }
}

#[test]
fn bad_command_lines_are_refused() {
    for args in [
        vec!["SONG", "-Nx"], // a value on a flag that takes none
        vec!["SONG", "-P1"],
        vec!["SONG", "-B2"], // on/off flags are 0 or 1
        vec!["SONG", "-B"],
        vec!["SONG", "-O7"],
        vec!["SONG", "-V3"],
        vec!["SONG", "--subtune", "300"], // past a byte
        vec!["SONG", "--subtune", "32"],  // past GoatTracker's 32 subtunes
        vec!["SONG", "--frames", "x"],
        vec!["SONG", "--frames"],
        vec!["SONG", "-W100"], // a page is a byte
        vec!["SONG", "-Z100"],
        vec!["SONG", "-A10000"],
        vec!["SONG", "-L10000"],
        vec!["SONG", "-S17"], // gt2reloc's limit is 16
        vec!["SONG", "-Gx"],
        vec!["SONG", "-G-1"],
        vec!["SONG", "-X1"], // no such option
        vec!["SONG", "--bogus"],
        vec!["SONG", "-"],
        vec!["SONG", "SONG"], // two songs
        vec![],               // none
    ] {
        let (ok, out) = gt2_trace(&args);
        assert!(!ok, "{args:?} was taken: {}", &out[..out.len().min(200)]);
        assert!(out.contains("gt2-trace:"), "{args:?}: {out}");
    }
}
