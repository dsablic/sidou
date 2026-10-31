// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::process::ExitCode;

const DEFAULT_TESTS_PER_OPCODE: usize = 100;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: harte-subset <json dir> <out dir> [count]");
        return ExitCode::FAILURE;
    }
    let count = match args.get(2).map(|s| s.parse::<usize>()) {
        None => DEFAULT_TESTS_PER_OPCODE,
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
