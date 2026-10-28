// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! No list is a dead end: every list the menu opens (the menu's own lines, SETTINGS, SONG's
//! actions and its LOAD, DELETE and SAVE AS pages, the SIDS browser) takes HOME and SAVE, alone
//! and in pairs, without a crash, and HOME then leads back to LIVE within a few presses with a
//! key that still sounds. Needs the fm1 build (SIDOU_FM1_BUILD); skips without it.

use ergo::device::Dev;
use ergo::journeys::run_script;
use ergo::metrics::rms_dbfs;
use ergo::screen::text_runs;

/// HOME presses allowed from any state back to LIVE (the deepest stack is SONG's LOAD, a
/// confirmation, and the menu: 4).
const HOME_LIMIT: usize = 8;

/// What a key press does on a list, as the script that runs it.
const ACTIONS: [(&str, &str); 4] = [
    ("HOME", "press HOME; wait 500"),
    ("SAVE", "press SAVE; wait 800"),
    ("SAVE SAVE", "press SAVE; wait 800; press SAVE; wait 800"),
    ("SAVE HOME", "press SAVE; wait 800; press HOME; wait 500"),
];

/// SONG's root with its cursor on LOAD: the firmware remembers the cursor, SELECT clamps.
const SONG_ROOT: &str = "select-item SONG; press SAVE; wait 800; turn SELECT -5; wait 100";

/// One list: the script that reaches it from the menu ("@;rest": SONG's root, then rest), the
/// view it must be and the text it must show (a page reached by the wrong turn would pass HOME
/// and SAVE all the same), and whether the machine's store holds a song of the player's own.
struct List {
    name: &'static str,
    reach: &'static str,
    view: &'static str,
    text: Option<&'static str>,
    own_song: bool,
}

const fn list(
    name: &'static str,
    reach: &'static str,
    view: &'static str,
    text: Option<&'static str>,
) -> List {
    List {
        name,
        reach,
        view,
        text,
        own_song: false,
    }
}

fn lists() -> Vec<List> {
    let delete = "@;turn SELECT 3; press SAVE; wait 1500";
    vec![
        list("MENU", "", "menu", Some("SONG")),
        list(
            "CHIP",
            "select-item CHIP; press SAVE; wait 3000",
            "chip",
            None,
        ),
        list(
            "TRACKER",
            "select-item TRACKER; press SAVE; wait 800",
            "tracker",
            None,
        ),
        list(
            "SCENE",
            "select-item SCENE; press SAVE; wait 3000",
            "scene",
            None,
        ),
        list("SONG", SONG_ROOT, "song_menu", Some("LOAD")),
        list(
            "SONG LOAD",
            "@;press SAVE; wait 1500",
            "song_menu",
            Some("SIDOU LOOP"),
        ),
        list(
            "SONG NEW",
            "@;turn SELECT 1; press SAVE; wait 800",
            "song_menu",
            None,
        ),
        list(
            "SONG SAVE AS",
            "@;turn SELECT 2; press SAVE; wait 1000",
            "song_menu",
            Some("CURSOR"),
        ),
        list(
            "SONG DELETE, none of one's own",
            delete,
            "song_menu",
            Some("NO SONGS"),
        ),
        List {
            own_song: true,
            ..list("SONG DELETE", delete, "song_menu", Some("CHROME COMET"))
        },
        List {
            own_song: true,
            ..list(
                "SONG DELETE confirm",
                "@;turn SELECT 3; press SAVE; wait 1500; press SAVE; wait 800",
                "song_menu",
                None,
            )
        },
        list(
            "SIDS",
            "select-item SIDS; press SAVE; wait 1500",
            "sid_view_browser",
            None,
        ),
        list(
            "SETTINGS",
            "select-item SETTINGS; press SAVE; wait 800",
            "settings",
            None,
        ),
        list(
            "STATUS",
            "select-item STATUS; press SAVE; wait 800",
            "status",
            None,
        ),
        list(
            "MIDI",
            "select-item MIDI; press SAVE; wait 800",
            "midi",
            None,
        ),
    ]
}

/// HOME until LIVE, as a player lost in a list would: the views passed on the way, or an error.
fn home_to_live(d: &mut Dev) -> Result<Vec<String>, Vec<String>> {
    let mut seen = vec![d.view()];
    for _ in 0..HOME_LIMIT {
        if d.view() == "live" {
            return Ok(seen);
        }
        run_script(d, "press HOME; wait 700");
        seen.push(d.view());
    }
    if d.view() == "live" {
        Ok(seen)
    } else {
        Err(seen)
    }
}

fn screen_text(d: &mut Dev) -> String {
    let shot = d.shot();
    text_runs(&shot.cells)
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn key_sounds(d: &mut Dev) -> bool {
    let from = d.audio_mark();
    d.note(ergo::device::key_of_note(69), 300);
    rms_dbfs(&d.audio_since(from)) > -60.0
}

#[test]
fn every_list_takes_save_and_home_and_leads_back_to_live() {
    let Some(build) = ergo::common::build_dir() else {
        ergo::common::skip_message("the list walk");
        return;
    };
    // One machine per list (side by side on every core), its four tries one after the other,
    // each from wherever the last left the firmware (a song loaded, a tune playing), as a
    // player's next try would.
    let all = lists();
    let problems: Vec<String> = ergo::par_map(&all, |l| {
        let (list, reach, view, text) = (l.name, l.reach, l.view, &l.text);
        let mut d = Dev::power_on_with(
            &build,
            &["chrome-comet".to_string()][..usize::from(l.own_song)],
        );
        ergo::common::run_past_boot(&mut d.m, &d.fw);
        d.run(300);
        d.mid_frame();
        let mut problems: Vec<String> = Vec::new();
        for (action, script) in ACTIONS {
            let case = format!("{list} then {action}");
            if let Err(seen) = home_to_live(&mut d) {
                problems.push(format!("{case}: could not start from LIVE: {seen:?}"));
                continue;
            }
            run_script(&mut d, "press HOME; wait 600");
            if d.view() != "menu" {
                problems.push(format!("{case}: HOME from LIVE reached {}", d.view()));
                continue;
            }
            // "@;rest": SONG's root first, then rest
            match reach.strip_prefix("@;") {
                Some(rest) => run_script(&mut d, &format!("{SONG_ROOT}; {rest}")),
                None if !reach.is_empty() => run_script(&mut d, reach),
                None => {}
            }
            let on_screen = screen_text(&mut d);
            if d.view() != view || text.is_some_and(|t| !on_screen.contains(t)) {
                problems.push(format!(
                    "{case}: reached {} showing {on_screen:?}, wanted {view} with {text:?}",
                    d.view()
                ));
                continue;
            }
            let at = d.view();
            run_script(&mut d, script);
            let after = d.view();
            println!("{case}: {at} -> {after}");
            match home_to_live(&mut d) {
                Err(seen) => problems.push(format!(
                    "{case}: {at} ... HOME x{HOME_LIMIT} never reached LIVE: {seen:?}"
                )),
                Ok(_) if !key_sounds(&mut d) => {
                    problems.push(format!("{case}: back on LIVE, but A4 is silent"));
                }
                Ok(_) => {}
            }
        }
        problems
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
