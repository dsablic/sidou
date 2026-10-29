// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Compiles the C core, the workload and the portable app logic (`app/`) with `cc` and generates their FFI with bindgen.
//! `wrap_static_fns` makes the header-only `static inline` functions callable: bindgen writes
//! an extern wrapper for each into a C file, which is compiled with the rest.

use std::env;
use std::path::{Path, PathBuf};

const SOURCES: &[&str] = &[
    "core/sid/sid.c",
    "core/sid/sid_shadow.c",
    "core/sid/sid_character.c",
    "core/sid/sid_dac.c",
    "core/fx/fx.c",
    "core/fx/fx_slots.c",
    "core/fx/fx_slots_init.c",
    "core/fx/fx_reverb.c",
    "core/fx/fx_plate.c",
    "core/fx/fx_limit.c",
    "core/sid/sid_filter.c",
    "core/song/gt2_song.c",
    "core/song/gt2_sng.c",
    "core/song/gt2_stream.c",
    "core/song/gt2_ins.c",
    "core/song/gt2_kit.c",
    "core/song/song_ext.c",
    "core/psid/psid_file.c",
    "core/psid/psid_env.c",
    "core/psid/psid_ctl.c",
    "core/psid/psid_ctl_isr.c",
    "core/player/gt2_play.c",
    "core/player/gt2_shape.c",
    "core/player/song_ctl.c",
    "core/cpu6502/cpu6502.c",
    "bench/workload.c",
    "app/about.c",
    "app/boot.c",
    "app/bootguard.c",
    "app/chip_ctl.c",
    "app/chip_mix.c",
    "app/chip_spread.c",
    "app/chip_view.c",
    "app/credits.c",
    "app/demo_song.c",
    "app/font.c",
    "app/fx_defaults.c",
    "app/fx_live.c",
    "app/grid.c",
    "app/help_view.c",
    "app/keys_route.c",
    "app/lab_draw.c",
    "app/lab_edit.c",
    "app/lab_sound.c",
    "app/lab_view.c",
    "app/live.c",
    "app/learn_view.c",
    "app/live_cutoff.c",
    "app/live_dice.c",
    "app/live_setup.c",
    "app/live_freq.c",
    "app/live_view.c",
    "app/master_gain.c",
    "app/logo.c",
    "app/logo_bitmap.c",
    "app/midi_clock.c",
    "app/midi_in.c",
    "app/midi_learn.c",
    "app/midi_map.c",
    "app/midi_out.c",
    "app/midi_ring.c",
    "app/midi_route.c",
    "app/midi_sync.c",
    "app/midi_rx.c",
    "app/midi_usb.c",
    "app/panel.c",
    "app/reg_ring.c",
    "app/scene.c",
    "app/scene_art.c",
    "app/scene_px.c",
    "app/scene_rmr.c",
    "app/scene_rmr_art.c",
    "app/scene_song.c",
    "app/scene_view.c",
    "app/save_key.c",
    "app/scope.c",
    "app/tail_gate.c",
    "app/thump_out.c",
    "app/screen.c",
    "app/settings.c",
    "app/sine.c",
    "app/song_view.c",
    "app/song_pages.c",
    "app/sid_io.c",
    "app/sid_view.c",
    "app/song_io.c",
    "app/song_edit.c",
    "app/step_view.c",
    "app/store.c",
    "app/sysex.c",
    "app/theme.c",
    "app/title_card.c",
    "app/title_card_bitmap.c",
    "app/tracker_view.c",
    "app/ui_sound.c",
    "app/user_profile.c",
    "app/user_sound.c",
    "app/views.c",
    "app/widget.c",
    "bench/workload_fx.c",
    "bench/workload_slots.c",
    "bench/workload_scene.c",
];

/// Include roots, relative to the repository: core headers are "sid/...", the others bare.
const INCLUDES: [&str; 3] = ["core", "bench", "app"];

/// The CC/NRPN map's text for the generated reference (app/midi_map.h); the device leaves it
/// out.
const MIDI_MAP_DOC: &str = "SIDOU_MIDI_MAP_DOC";

/// The register ring's test hook (app/reg_ring.h): the tests play the audio interrupt inside
/// the producer's choice of a free DAC table. The device leaves it out.
const RACE_HOOK: &str = "SIDOU_REG_RING_RACE_HOOK";

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
        .define(RACE_HOOK, None)
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
        root.join("core/song").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("core/player").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("core/cpu6502").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("core/psid").display()
    );
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
        .clang_arg(format!("-D{RACE_HOOK}"))
        .allowlist_function(
            concat!(
                "sid_.*|sidou_.*|cpu6502_.*|psid_.*|gt2_.*|song_.*|workload_.*|live_.*|midi_.*|panel_.*|reg_ring_.*|bootguard_.*|",
                "fx_live.*|grid_.*|screen_.*|menu_.*|widget_.*|views_.*|logo_.*|sine_.*|boot_.*|about_.*|",
                "learn_view_.*|scope_.*|scene_.*|title_card_.*|chip_view_.*|chip_mix|chip_spread_.*|master_.*|tail_gate_.*|thump_out_.*|chip_ctl_.*|chip_setup_.*|",
                "keys_route_.*|ui_sound_.*|store_.*|sysex_.*|save_key_.*|settings_.*|step_view_.*|tracker_view_.*|user_profile_.*|user_profiles_.*|theme_.*|help_view_.*|user_sound_.*|user_sounds_.*|",
                "sid_view_.*|lab_.*",
            ),
        )
        .allowlist_type(
            "sid.*|cpu6502.*|psid.*|gt2_.*|song_.*|live.*|midi_.*|panel.*|reg_ring.*|bootguard|fx_live.*|grid.*|screen.*|view.*|menu.*|logo.*|credits.*|boot.*|about.*|learn_view.*|scope.*|scene.*|title_card.*|chip_view.*|chip_mix.*|chip_spread.*|master.*|tail_gate.*|thump_out.*|chip_ctl.*|chip_setup.*|keys_route.*|ui_sound.*|store.*|save_key.*|settings.*|step_view.*|user_profile.*|user_sound.*|theme.*|help_.*|sid_view.*|sid_status|sid_transport|lab_.*",
        )
        .allowlist_var(
            concat!(
                "SID_.*|SIDOU_.*|CPU6502_.*|PSID_.*|GT2_.*|SONG_.*|WORKLOAD_.*|LIVE_.*|MIDI_.*|PANEL_.*|REG_RING_.*|BOOTGUARD_.*|",
                "live_.*|FONT_.*|font_.*|GRID_.*|grid_.*|SCREEN_.*|screen_.*|MENU_.*|VIEWS_.*|views_.*|",
                "LOGO_.*|logo_.*|CREDITS_.*|credits_.*|SINE_.*|BOOT_.*|ABOUT_.*|LEARN_VIEW_.*|SCOPE_.*|",
                "SCENE_.*|scene_.*|TITLE_CARD.*|title_card_.*|CHIP_VIEW_.*|chip_view_.*|CHIP_MIX_.*|CHIP_SPREAD_.*|MASTER_.*|",
                "TAIL_.*|THUMP_OUT_.*|UI_SOUND_.*|KEYS_ROUTE_.*|SYSEX_.*|HELP_.*|help_.*|",
                "demo_song_.*|gt2_kit_.*|song_view_.*|song_pages_.*|STORE_.*|SAVE_KEY_.*|SETTINGS_.*|step_view_.*|STEP_VIEW_.*|tracker_view_.*|TRACKER_VIEW_.*|",
                "USER_PROFILE.*|user_profile.*|USER_SOUND.*|THEME_.*|theme_.*|SID_VIEW_.*|SID_T_.*|sid_view_.*|LAB_.*|lab_.*",
            ),
        )
        // `static const` tables private to the headers; they have no symbol to link to.
        .blocklist_item("SID_ENV_RATE_POS|SID_ENV_THRESHOLDS|SID_PD|SID_NOISE_WB")
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
    // the firmware's chips, MIDI out's state, its MIDI in ring, the LIVE view's tap, the
    // views' title card and the song transport from its RAM. The host's own layout differs
    // wherever a struct holds a pointer. bindgen's layout assertions carry clang's numbers for
    // the target.
    let ilp32 = bindgen::Builder::default()
        .header(wrapper.to_str().unwrap())
        .clang_args(INCLUDES.map(|dir| format!("-I{}", root.join(dir).display())))
        .clang_arg("-std=c11")
        .clang_arg("--target=i686-unknown-none")
        .clang_arg("-ffreestanding")
        .allowlist_type(concat!(
            "sid|sid_osc|sid_env|sid_shadow|reg_ring|reg_ring_entry|reg_ring_chips|reg_ring_out|keys_route|",
            "chip_ctl|chip_setup|midi_out|midi_ring|midi_ring_entry|midi_learn|live|scope|grid|views|",
            "title_card|help_view|song_ctl|screen|store|store_writer|store_file_writer|boot|gt2_stream|",
            "midi_route|midi_sync|midi_clock|sysex|psid_env|psid_ctl|psid_work|sid_view|song_view",
        ))
        .generate()
        .expect("bindgen (ILP32 layout) failed")
        .to_string();
    std::fs::write(out.join("ilp32.rs"), ilp32_layout(&ilp32)).expect("write ilp32.rs");

    c_build(&root)
        .files(SOURCES.iter().map(|s| root.join(s)))
        .file(static_fns.with_extension("c"))
        .include(&manifest)
        // The 6502's device variant (core/cpu6502/cpu6502.h); nothing else reads it.
        .define("CPU6502_SMALL", None)
        .compile("sidou");

    // The 6502 again for the tests only: every operand access through the hooks
    // (CPU6502_HOOK_ALL), so Tom Harte's tests check each one's cycle stamp. Once as the
    // device builds it (the I/O accesses out of line) and once with them inline, under
    // their own names.
    let cpu = root.join("core/cpu6502/cpu6502.c");
    for (name, small) in [
        ("cpu6502_run_hook_all", true),
        ("cpu6502_run_hook_inline", false),
    ] {
        let mut build = c_build(&root);
        build
            .file(&cpu)
            .define("CPU6502_HOOK_ALL", None)
            .define("cpu6502_run", name)
            .define("cpu6502_reset_regs", format!("{name}_reset_regs").as_str());
        if small {
            build.define("CPU6502_SMALL", None);
        }
        build.compile(name);
    }

    // The chip again for the mute's reference (core/sid/sid.c, SID_TEST_ZERO_VOICE): the
    // voices sid_test_set_zero_voices names output 0 (on the calling thread). Each of its
    // symbols under a `_zero_voice` name, beside the real one; it shares the other files'
    // (the DACs, the filter curves).
    let mut zero = c_build(&root);
    zero.file(root.join("core/sid/sid.c"))
        .define("SID_TEST_ZERO_VOICE", None);
    for symbol in ZERO_VOICE_SYMBOLS {
        zero.define(symbol, format!("{symbol}_zero_voice").as_str());
    }
    zero.compile("sid_zero_voice");
}

/// core/sid/sid.c's external symbols, renamed in its zero-voice build (above).
const ZERO_VOICE_SYMBOLS: &[&str] = &[
    "SID_NOISE_JUMP",
    "SID_NOISE_WB",
    "SID_NOISE_WB_BYTE",
    "SID_PD_ROWS",
    "sid_advance",
    "sid_base_output_gain",
    "sid_clock",
    "sid_combined_table",
    "sid_external_coefficients",
    "sid_filter_refresh",
    "sid_filter_row_part",
    "sid_init",
    "sid_init_begin",
    "sid_init_end",
    "sid_noise_tables_init",
    "sid_nonlin_for",
    "sid_nonlin_for_rows",
    "sid_read",
    "sid_render",
    "sid_render_tap",
    "sid_set_combined_profile",
    "sid_set_dac",
    "sid_set_filter_blend",
    "sid_set_filter_curve",
    "sid_set_filter_nonlinearity",
    "sid_set_filter_toward",
    "sid_set_gate_reset",
    "sid_write",
];
