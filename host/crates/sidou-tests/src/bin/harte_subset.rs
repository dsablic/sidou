// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Writes the committed subset of Tom Harte's 6502 tests: the first tests of each opcode's
//! JSON file, in the binary `sidou_tests::harte` reads (third_party/6502-tests/VENDORED.md).
//!
//! `cargo run --release -p sidou-tests --bin harte-subset -- <json dir> <out dir> [count]`
//! (count 100 by default).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: harte-subset <json dir> <out dir> [count]");
        return ExitCode::FAILURE;
    }
    let count = match args.get(2).map(|s| s.parse::<usize>()) {
        None => 100,
        Some(Ok(n)) => n,
        Some(Err(e)) => {
            eprintln!("count: {e}");
            return ExitCode::FAILURE;
        }
    };
    match sidou_tests::harte::convert(&PathBuf::from(&args[0]), &PathBuf::from(&args[1]), count) {
        Ok(bytes) => {
            println!("256 files, {count} tests each, {bytes} bytes");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("harte-subset: {e}");
            ExitCode::FAILURE
        }
    }
}
