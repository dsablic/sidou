// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `sidou-sysex sid FILE.sid... [--out DIR] [--trs]`: one `.syx` per tune, each the tune's
//! upload to the device's store (docs/reference/sysex.md: PUT, DATA, END, one SysEx message a
//! packet), named from its title (sidou_sysex::sid::store_name). A tune the device would refuse
//! is reported with the device's reason and written nowhere. The `.syx` goes to DIR (the current
//! directory by default) as the `.sid`'s own file name with `.syx`. This program opens no MIDI
//! port: a SysEx tool sends the file (docs/manual/sid-tunes.md).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sidou_sysex::sid;

const USAGE: &str = "usage: sidou-sysex sid FILE.sid... [--out DIR] [--trs]";

struct Args {
    files: Vec<PathBuf>,
    out: PathBuf,
    trs: bool,
}

fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    if it.next().as_deref() != Some("sid") {
        return Err(USAGE.to_string());
    }
    let mut a = Args {
        files: Vec::new(),
        out: PathBuf::from("."),
        trs: false,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => a.out = it.next().ok_or(USAGE)?.into(),
            "--trs" => a.trs = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ => a.files.push(arg.into()),
        }
    }
    if a.files.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(a)
}

/// The `.syx`'s path for `file`: DIR and its file name, `-2`, `-3`, ... on a clash in this run.
fn target(out: &Path, file: &Path, used: &mut HashSet<PathBuf>) -> PathBuf {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tune".to_string());
    let mut path = out.join(format!("{stem}.syx"));
    let mut k = 2;
    while used.contains(&path) {
        path = out.join(format!("{stem}-{k}.syx"));
        k += 1;
    }
    used.insert(path.clone());
    path
}

fn main() -> ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let mut names = HashSet::new();
    let mut paths = HashSet::new();
    let mut failed = 0;
    let mut frames = 0;
    for file in &a.files {
        let shown = file.display();
        let bytes = match std::fs::read(file) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{shown}: {e}");
                failed += 1;
                continue;
            }
        };
        let h = match sid::check(&bytes) {
            Ok(h) => h,
            Err(why) => {
                eprintln!("{shown}: refused: {why}");
                failed += 1;
                continue;
            }
        };
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        let name = sid::store_name(&sid::title(&h), &stem, &mut names);
        let syx = sid::syx(&name, &bytes).expect("a checked tune's frames");
        let path = target(&a.out, file, &mut paths);
        if let Err(e) = std::fs::write(&path, &syx) {
            eprintln!("{}: {e}", path.display());
            failed += 1;
            continue;
        }
        let packets = bytes.len().div_ceil(sidou_sysex::MAX_DATA_LEN) + 2;
        frames += packets;
        println!(
            "{shown} -> {} as {name}: {} by {}, {} song{}, {} chip{}; {packets} messages",
            path.display(),
            sid::title(&h),
            sid::author(&h),
            h.songs,
            if h.songs == 1 { "" } else { "s" },
            if h.sid2 != 0 { 2 } else { 1 },
            if h.sid2 != 0 { "s" } else { "" },
        );
        for w in sid::warnings(&h) {
            println!("  note: {w}");
        }
    }
    if frames > 0 {
        // docs/reference/sysex.md, "Pacing and transactions".
        if a.trs {
            println!(
                "TRS: pause one second after every message (about {} s for these)",
                frames
            );
        } else {
            println!(
                "USB: send each message once the device has answered the last (ACK), or pause \
                 one second after each; over TRS, run with --trs"
            );
        }
    }
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
