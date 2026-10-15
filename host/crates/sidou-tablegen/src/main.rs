// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Writes the generated tables into core/sid/gen/: all of them, or with `fit-filter` the
//! filter's alone (gen/filter.h), printing how well the fit matches reSIDfp.

use sidou_tablegen::Generated;

fn write(g: &Generated) {
    let path = sidou_tablegen::repo_root().join(g.path);
    std::fs::create_dir_all(path.parent().unwrap()).expect("create core/sid/gen");
    std::fs::write(&path, &g.contents).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!("wrote {} ({} bytes)", g.path, g.contents.len());
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        None => {
            for g in sidou_tablegen::generate() {
                write(&g);
            }
        }
        Some("fit-filter") => {
            let fits = sidou_tablegen::filter_fits();
            for f in &fits {
                println!("{}", f.report());
            }
            write(&sidou_tablegen::filter_header(&fits));
        }
        Some(other) => {
            eprintln!("usage: sidou-tablegen [fit-filter] (unknown argument {other})");
            std::process::exit(2);
        }
    }
}
