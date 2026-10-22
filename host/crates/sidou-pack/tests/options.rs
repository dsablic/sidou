// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The build-option record (platform/fm1/build_options.c): its reading, what differs from a
//! release's defaults, and that the defaults agree with the CMake files' `option()` lines and
//! `sidou_option_defaults`. Needs no inputs.

use sidou_pack::{build_options, option_problems, DEFAULT_OPTIONS};

fn record(edit: impl Fn(&str, &str) -> String) -> Vec<u8> {
    let mut r = b"..SIDOU-OPTIONS 1".to_vec();
    for (k, v) in DEFAULT_OPTIONS {
        r.extend_from_slice(format!(" {k}={}", edit(k, v)).as_bytes());
    }
    r.extend_from_slice(b" SIDOU_PRODUCT=FM-1_921\0tail");
    r
}

#[test]
fn the_defaults_pass_whatever_the_product() {
    let o = build_options(&record(|_, v| v.to_owned())).unwrap();
    assert_eq!(o.len(), DEFAULT_OPTIONS.len() + 1);
    assert!(option_problems(&o).is_empty());
}

#[test]
fn every_option_off_its_default_is_named() {
    for (name, _) in DEFAULT_OPTIONS {
        let o = build_options(&record(|k, v| {
            if k == name {
                format!("{v}x")
            } else {
                v.to_owned()
            }
        }))
        .unwrap();
        let p = option_problems(&o);
        assert_eq!(p.len(), 1, "{name}: {p:?}");
        assert!(p[0].contains(name), "{name}: {p:?}");
    }
}

#[test]
fn a_missing_record_or_option_is_not_a_pass() {
    assert!(build_options(b"no record here").is_none());
    assert!(
        build_options(b"SIDOU-OPTIONS 1 A=B").is_none(),
        "unterminated"
    );
    let o = build_options(b"SIDOU-OPTIONS 1 SIDOU_USB_FIX=ON\0").unwrap();
    assert_eq!(option_problems(&o).len(), DEFAULT_OPTIONS.len() - 1);
}

/// The `sidou_option_defaults` list in platform/fm1/CMakeLists.txt and the `option()` and
/// cached `set()` lines of the CMake files, against `DEFAULT_OPTIONS`: a new option or default
/// in one place and not the others fails here.
#[test]
fn the_defaults_agree_with_the_cmake_files() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let read =
        |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let fm1 = read("platform/fm1/CMakeLists.txt");
    let at = fm1.find("set(sidou_option_defaults").unwrap();
    let list = &fm1[at..at + fm1[at..].find(')').unwrap()];
    let listed: Vec<(String, String)> = list
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|e| {
            let (k, v) = e.split_once('=').unwrap();
            (k.to_owned(), if v.is_empty() { "-" } else { v }.to_owned())
        })
        .filter(|(k, _)| k != "SIDOU_PRODUCT")
        .collect();
    let want: Vec<(String, String)> = DEFAULT_OPTIONS
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    assert_eq!(
        listed, want,
        "sidou_option_defaults and DEFAULT_OPTIONS differ"
    );

    // Every SIDOU_ option() and cached set() in the CMake files is in the list, with its default.
    for file in [
        "platform/fm1/CMakeLists.txt",
        "core/CMakeLists.txt",
        "app/CMakeLists.txt",
        "CMakeLists.txt",
    ] {
        for line in read(file).lines() {
            let line = line.trim();
            let found = if let Some(r) = line.strip_prefix("option(") {
                let name = r.split(' ').next().unwrap();
                let default = r.trim_end_matches(')').rsplit(' ').next().unwrap();
                Some((name.to_owned(), default.to_owned()))
            } else if let Some(r) = line.strip_prefix("set(SIDOU_") {
                let (name, rest) = r.split_once(' ').unwrap();
                rest.contains("CACHE").then(|| {
                    (
                        format!("SIDOU_{name}"),
                        rest.split('"').nth(1).unwrap().to_owned(),
                    )
                })
            } else {
                None
            };
            let Some((name, default)) = found else {
                continue;
            };
            if name == "SIDOU_PRODUCT" {
                continue;
            }
            let default = if default.is_empty() {
                "-".to_owned()
            } else {
                default
            };
            assert!(
                want.contains(&(name.clone(), default.clone())),
                "{file}: {name} (default {default}) is not in DEFAULT_OPTIONS: add it to it and to \
                 sidou_option_defaults"
            );
        }
    }
}
