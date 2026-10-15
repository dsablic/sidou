// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Compiles the C core, the workload and the portable app logic (`app/`) with `cc` and generates their FFI with bindgen.
//! `wrap_static_fns` makes the header-only `static inline` functions callable: bindgen writes
//! an extern wrapper for each into a C file, which is compiled with the rest.

use std::env;
use std::path::{Path, PathBuf};

const SOURCES: [&str; 42] = [
    "core/sid/sid.c",
    "core/sid/sid_character.c",
    "core/sid/sid_dac.c",
    "core/fx/fx.c",
    "core/fx/fx_slots.c",
    "core/fx/fx_reverb.c",
    "core/fx/fx_plate.c",
    "core/sid/sid_filter.c",
    "bench/workload.c",
    "app/about.c",
    "app/boot.c",
    "app/bootguard.c",
    "app/credits.c",
    "app/font.c",
    "app/fx_live.c",
    "app/grid.c",
    "app/live.c",
    "app/learn_view.c",
    "app/live_freq.c",
    "app/live_view.c",
    "app/logo.c",
    "app/logo_bitmap.c",
    "app/midi_in.c",
    "app/midi_learn.c",
    "app/midi_map.c",
    "app/midi_out.c",
    "app/midi_ring.c",
    "app/midi_rx.c",
    "app/midi_usb.c",
    "app/panel.c",
    "app/reg_ring.c",
    "app/scene.c",
    "app/scene_view.c",
    "app/scope.c",
    "app/screen.c",
    "app/sine.c",
    "app/title_card.c",
    "app/title_card_bitmap.c",
    "app/views.c",
    "app/widget.c",
    "bench/workload_fx.c",
    "bench/workload_slots.c",
];

/// Include roots, relative to the repository: core headers are "sid/...", the others bare.
const INCLUDES: [&str; 3] = ["core", "bench", "app"];

/// The CC/NRPN map's text for the generated reference (app/midi_map.h); the device leaves it
/// out.
const MIDI_MAP_DOC: &str = "SIDOU_MIDI_MAP_DOC";

include!("../../build-support/sanitize.rs");

/// `pub mod <struct> { pub const SIZE: usize; pub const <field>: usize; }` for every
/// struct, from bindgen's layout assertions:
/// `["Size of sid"][size_of::<sid>() - 8728usize]` and
/// `["Offset of field: sid::fc"][offset_of!(sid, fc) - 1132usize]`.
fn ilp32_layout(bindings: &str) -> String {
    let mut structs: Vec<(String, Vec<(String, String)>)> = Vec::new();
    // Each assertion: `["<label>"][<expression> - <n>usize]`, possibly over several lines.
    for chunk in bindings.split("[\"").skip(1) {
        let Some((label, rest)) = chunk.split_once("\"]") else {
            continue;
        };
        let Some(end) = rest.find("usize]") else {
            continue;
        };
        let value = rest[..end].rsplit('-').next().unwrap().trim().to_string();
        if let Some(name) = label.strip_prefix("Size of ") {
            structs.push((name.to_string(), vec![("SIZE".into(), value)]));
        } else if let Some(path) = label.strip_prefix("Offset of field: ") {
            let (owner, field) = path.split_once("::").unwrap();
            let s = structs
                .iter_mut()
                .find(|s| s.0 == owner)
                .expect("size before offsets");
            s.1.push((field.to_string(), value));
        }
    }
    let mut out = String::new();
    for (name, fields) in structs {
        out += &format!("pub mod {name} {{\n");
        for (field, value) in fields {
            out += &format!("    pub const {field}: usize = {value};\n");
        }
        out += "}\n";
    }
    out
}

fn c_build(root: &Path) -> cc::Build {
    let mut build = cc::Build::new();
    for dir in INCLUDES {
        build.include(root.join(dir));
    }
    build
        .flag("-std=c11")
        // Float results must be identical on host and pi32v2 so their outputs can be compared.
        .flag("-ffp-contract=off")
        // The host tests check every combined-waveform profile of both models; the firmware
        // has the 6581's three and the 8580's AVERAGE (core/CMakeLists.txt), and a profile
        // not compiled in plays AVERAGE.
        .define("SIDOU_ALL_COMBINED_PROFILES", None)
        .define("SIDOU_ALL_COMBINED_PROFILES_8580", None)
        .warnings(true)
        .extra_warnings(true)
        .flag("-Wshadow")
        .define(MIDI_MAP_DOC, None)
        .warnings_into_errors(true);
    sanitize::apply_sanitizer(&mut build);
    build
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../..").canonicalize().unwrap();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let wrapper = manifest.join("wrapper.h");

    println!("cargo:rerun-if-changed={}", wrapper.display());
    println!("cargo:rerun-if-changed={}", root.join("core/sid").display());
    println!("cargo:rerun-if-changed={}", root.join("core/fx").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bench/workload.h").display()
    );
    println!("cargo:rerun-if-changed={}", root.join("app").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bench/workload_fx.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("bench/workload_slots.h").display()
    );
    for s in SOURCES {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }

    let static_fns = out.join("static_fns");
    let bindings = bindgen::Builder::default()
        .header(wrapper.to_str().unwrap())
        .clang_args(INCLUDES.map(|dir| format!("-I{}", root.join(dir).display())))
        .clang_arg("-std=c11")
        .clang_arg("-DSIDOU_ALL_COMBINED_PROFILES")
        .clang_arg("-DSIDOU_ALL_COMBINED_PROFILES_8580")
        .clang_arg(format!("-D{MIDI_MAP_DOC}"))
        .allowlist_function(
            "sid_.*|sidou_.*|workload_.*|live_.*|midi_.*|panel_.*|reg_ring_.*|bootguard_.*|fx_live.*|grid_.*|screen_.*|menu_.*|widget_.*|views_.*|logo_.*|sine_.*|boot_.*|about_.*|learn_view_.*|scope_.*|scene_.*|title_card_.*",
        )
        .allowlist_type(
            "sid.*|live.*|midi_.*|panel.*|reg_ring.*|bootguard|fx_live.*|grid.*|screen.*|view.*|menu.*|logo.*|credits.*|boot.*|about.*|learn_view.*|scope.*|scene.*|title_card.*",
        )
        .allowlist_var(
            "SID_.*|SIDOU_.*|WORKLOAD_.*|LIVE_.*|MIDI_.*|PANEL_.*|REG_RING_.*|BOOTGUARD_.*|live_.*|FONT_.*|font_.*|GRID_.*|grid_.*|SCREEN_.*|screen_.*|MENU_.*|VIEWS_.*|views_.*|LOGO_.*|logo_.*|CREDITS_.*|credits_.*|SINE_.*|BOOT_.*|ABOUT_.*|LEARN_VIEW_.*|SCOPE_.*|SCENE_.*|scene_.*|TITLE_CARD.*|title_card_.*",
        )
        // `static const` tables private to the headers; they have no symbol to link to.
        .blocklist_item("SID_ENV_RATE_POS|SID_ENV_THRESHOLDS|SID_PULLDOWN_.*|SID_NOISE_WB")
        .prepend_enum_name(false)
        .derive_default(true)
        .derive_debug(true)
        .wrap_static_fns(true)
        .wrap_static_fns_path(&static_fns)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen failed; it needs libclang: install clang or Xcode, or set LIBCLANG_PATH");
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("write bindings.rs");

    // Field offsets and sizes of the structs as the pi32v2 lays them out (ILP32: 4-byte
    // pointers and ints; no 64-bit fields in them), for the emulator tests, which read
    // the firmware's chips, MIDI out's state, its MIDI in ring, the LIVE view's tap and the
    // views' title card from its RAM. The host's own layout differs wherever a struct holds a
    // pointer. bindgen's layout assertions carry clang's numbers for the target.
    let ilp32 = bindgen::Builder::default()
        .header(wrapper.to_str().unwrap())
        .clang_args(INCLUDES.map(|dir| format!("-I{}", root.join(dir).display())))
        .clang_arg("-std=c11")
        .clang_arg("--target=i686-unknown-none")
        .clang_arg("-ffreestanding")
        .allowlist_type(
            "sid|sid_osc|sid_env|reg_ring_chips|midi_out|midi_ring|midi_ring_entry|midi_learn|live|scope|views|title_card",
        )
        .generate()
        .expect("bindgen (ILP32 layout) failed")
        .to_string();
    std::fs::write(out.join("ilp32.rs"), ilp32_layout(&ilp32)).expect("write ilp32.rs");

    c_build(&root)
        .files(SOURCES.iter().map(|s| root.join(s)))
        .file(static_fns.with_extension("c"))
        .include(&manifest)
        .compile("sidou");
}
