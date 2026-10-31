// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
mod sanitize {
    const ASAN_COMMAND: &str = "SIDOU_SANITIZE=address RUSTFLAGS=-Zsanitizer=address \
         cargo +nightly test -Zbuild-std --target <host triple>";

    pub fn apply_sanitizer(build: &mut cc::Build) {
        println!("cargo:rerun-if-env-changed=SIDOU_SANITIZE");
        match std::env::var("SIDOU_SANITIZE").as_deref() {
            Err(_) | Ok("") => {}
            Ok("undefined") => {
                build.flag("-fsanitize=undefined");
                let trap = ["-fsanitize-trap=undefined", "-fsanitize-undefined-trap-on-error"]
                    .into_iter()
                    .find(|flag| build.is_flag_supported(flag).unwrap_or(false))
                    .expect("SIDOU_SANITIZE=undefined: the C compiler has no UBSan trap mode");
                build.flag(trap);
            }
            Ok("address") => {
                let rustflags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
                if !rustflags.split('\x1f').any(|f| f == "-Zsanitizer=address") {
                    panic!(
                        "SIDOU_SANITIZE=address needs the ASan runtime from Rust; run: {ASAN_COMMAND}"
                    );
                }
                build.flag("-fsanitize=address").flag("-fno-omit-frame-pointer");
            }
            Ok(other) => panic!("SIDOU_SANITIZE={other}: expected undefined or address"),
        }
    }
}
