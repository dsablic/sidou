// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Writes the generated tables into core/sid/gen/.

fn main() {
    let root = sidou_tablegen::repo_root();
    for g in sidou_tablegen::generate() {
        let path = root.join(g.path);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create core/sid/gen");
        std::fs::write(&path, &g.contents)
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        println!("wrote {} ({} bytes)", g.path, g.contents.len());
    }
}
