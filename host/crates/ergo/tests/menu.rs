// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `select-item` lands on the menu line it names, in both themes (needs the fm1 build,
//! SIDOU_FM1_BUILD): the firmware's own cursor and what the screen shows agree.

use ergo::device::{panel, Dev, FRAME_MS};
use ergo::journeys::{menu_selected, run_script};

const ITEMS: [&str; 12] = [
    "CHIP", "TRACKER", "SCENE", "SONG", "SIDS", "PRE", "OCTAVE", "SETTINGS", "STATUS", "MIDI",
    "CHARSET", "ABOUT",
];

fn walk(d: &mut Dev, theme: &str) {
    if d.view() != "menu" {
        run_script(d, "press HOME; wait 800");
    }
    run_script(d, "turn SELECT -20");
    d.run(FRAME_MS * 4);
    for (i, label) in ITEMS.iter().enumerate() {
        let (cursor, top) = d.menu_cursor();
        assert_eq!(usize::from(cursor), i, "{theme}: the cursor");
        d.run(FRAME_MS * 4);
        let shot = d.shot();
        let line = menu_selected(&shot, cursor, top).unwrap_or_default();
        assert!(line.starts_with(label), "{theme}: item {i} reads {line:?}");
        d.turn(panel::SELECT, 1);
        d.run(FRAME_MS * 4);
    }
}

#[test]
fn select_item_reaches_every_line_in_both_themes() {
    let Some(build) = ergo::common::build_dir() else {
        ergo::common::skip_message("the menu selection check");
        return;
    };
    let mut d = Dev::boot(&build);
    walk(&mut d, "default theme");
    for item in ["STATUS", "SCENE", "CHARSET", "ABOUT", "MIDI", "CHIP"] {
        run_script(&mut d, &format!("select-item {item}"));
        let (cursor, top) = d.menu_cursor();
        let line = menu_selected(&d.shot(), cursor, top).unwrap_or_default();
        assert!(line.starts_with(item), "{item}: {line:?}");
    }
    // SETTINGS (SAVE is the menu's OK) holds THEME.
    let before = d.settings_bytes();
    run_script(
        &mut d,
        "select-item SETTINGS; press SAVE; wait 400; select-item THEME; turn ALGORITHM 1; wait 400",
    );
    assert_eq!(d.view(), "settings");
    assert_ne!(d.settings_bytes(), before, "the theme turned");
    run_script(&mut d, "press HOME; wait 400");
    walk(&mut d, "second theme");
}
