// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Shared by the build scripts of sidou-sys and residfp-sys, which include!() it, so both
// read SIDOU_SANITIZE the same way.
//
// SIDOU_SANITIZE=undefined traps on UB without a runtime library, so it works on macOS and
// with stable Rust. Clang and newer GCC spell that -fsanitize-trap=undefined; GCC 12 only
// knows -fsanitize-undefined-trap-on-error.
// SIDOU_SANITIZE=address needs the Rust side built with -Zsanitizer=address (nightly),
// which provides the ASan runtime.

mod sanitize {
    const ASAN_COMMAND: &str = "SIDOU_SANITIZE=address RUSTFLAGS=-Zsanitizer=address \
         cargo +nightly test -Zbuild-std --target <host triple>";

    /// Adds the sanitizer flags SIDOU_SANITIZE asks for to `build`. Panics on an unknown value.
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
