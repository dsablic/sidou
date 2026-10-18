// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    let header = "../../../app/theme.h";
    println!("cargo:rerun-if-changed={header}");
    println!("cargo:rustc-check-cfg=cfg(theme_hint)");
    if std::fs::read_to_string(header)
        .expect("read firmware theme roles")
        .contains("uint8_t hint;")
    {
        println!("cargo:rustc-cfg=theme_hint");
    }
}
