// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{fs, path::PathBuf, process::Command};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn run(name: &str, source: &str) {
    let root = repo_root();
    let scratch = root.join("build/scratch");
    fs::create_dir_all(&scratch).unwrap();
    let file = scratch.join(format!("{name}-{}.c", std::process::id()));
    let exe = file.with_extension("test");
    fs::write(&file, source).unwrap();
    let build = Command::new("cc")
        .args(["-std=c11", "-O2", "-Werror", "-I"])
        .arg(root.join("app"))
        .arg("-I")
        .arg(root.join("core"))
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    let run = build
        .status
        .success()
        .then(|| Command::new(&exe).output().unwrap());
    fs::remove_file(&file).unwrap();
    if exe.exists() {
        fs::remove_file(&exe).unwrap();
    }
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = run.unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}
