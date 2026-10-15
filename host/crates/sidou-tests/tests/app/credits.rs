// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The About cracktro's credits: app/credits.c is generated from CREDITS.md
//! (sidou_tests::credits). SIDOU_CREDITS_UPDATE=1 rewrites it.

use std::ffi::CStr;

use sidou_sys::*;
use sidou_tests::credits::{self, Line, Page};

#[test]
fn the_generated_table_is_up_to_date() {
    let update = std::env::var("SIDOU_CREDITS_UPDATE").as_deref() == Ok("1");
    let stale = credits::check_or_write(&credits::load(), update);
    assert!(
        !stale,
        "app/credits.c is behind CREDITS.md; rerun with SIDOU_CREDITS_UPDATE=1"
    );
    for line in credits::c_source(&credits::load()).lines() {
        assert!(
            line.len() <= 100,
            "generated line over 100 columns: {line:?}"
        );
    }
}

fn text(p: *const std::ffi::c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned()
}

#[test]
fn the_compiled_table_is_the_file() {
    let want = credits::load();
    let n = unsafe { credits_page_count } as usize;
    assert_eq!(n, want.len());
    let pages = unsafe { std::slice::from_raw_parts(credits_pages.as_ptr(), n) };
    for (got, want) in pages.iter().zip(&want) {
        assert_eq!(text(got.heading), want.heading);
        let lines: Vec<Line> = got
            .line
            .iter()
            .take_while(|p| !p.is_null())
            .enumerate()
            .map(|(i, &p)| Line {
                text: text(p),
                name: got.names >> i & 1 != 0,
            })
            .collect();
        assert_eq!(lines, want.lines, "{}", want.heading);
    }
}

#[test]
fn every_page_fits_the_screen_in_the_font() {
    let pages = credits::load();
    assert!(pages.len() >= 6, "{} pages", pages.len());
    for p in &pages {
        assert!(!p.lines.is_empty() && p.lines.len() <= CREDITS_LINES as usize);
        for l in std::iter::once(&p.heading).chain(p.lines.iter().map(|l| &l.text)) {
            assert!(l.len() <= CREDITS_COLS as usize, "{l:?} is too long");
            assert!(l.chars().all(credits::in_font), "{l:?} is outside the font");
            assert!(!l.contains('?'), "{l:?} lost a character");
        }
    }
}

#[test]
fn everyone_in_the_file_is_on_a_page() {
    let all: String = credits::load()
        .iter()
        .flat_map(|p| p.lines.iter().map(|l| l.text.clone()))
        .collect::<Vec<_>>()
        .join(" ");
    for who in [
        "DAG LEM",
        "ANTTI S. LANKILA",
        "ANDREW SIMPER",
        "EMILIE GILLET",
        "JON DATTORRO",
        "FABIAN ESQUEDA AND VESA VALIMAKI",
        "UDO ZOLZER",
        "LEO KUROSHITA / HUGELTON INSTRUMENTS",
        "JOAKIM ERIKSSON",
        "PHILIP \"PEPTO\" TIMMERMANN",
        "LASSE OORNI (CADAVER)",
        "MIHALY HORVATH (HERMIT)",
        "M-VAVE",
    ] {
        assert!(all.contains(who), "{who:?} is missing: {all}");
    }
}

#[test]
fn bullets_split_into_who_and_what() {
    let pages = credits::parse(
        "<!-- SPDX -->\n# Credits\n\nIntro text.\n\n## Things\n\
         - **reSID**: Dag Lem\n- **Long**: one two\n  three\n- plain bullet\n",
    );
    let line = |text: &str, name| Line {
        text: text.into(),
        name,
    };
    assert_eq!(
        pages,
        [
            Page {
                heading: "CREDITS".into(),
                lines: vec![line("INTRO TEXT.", false)],
            },
            Page {
                heading: "THINGS".into(),
                lines: vec![
                    line("RESID", true),
                    line("DAG LEM", false),
                    line("LONG", true),
                    line("ONE TWO THREE", false),
                    line("PLAIN BULLET", false),
                ],
            },
        ]
    );
}

#[test]
fn a_bullet_moves_to_a_new_page_rather_than_split() {
    let md = "## S\n- **A**: 1\n- **B**: 2\n- **C**: 3\n- **D**: 4\n- **E**: x y z\n";
    let pages = credits::parse(md);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].lines.len(), 8);
    assert_eq!(pages[1].lines[0].text, "E");
}

#[test]
fn text_is_folded_and_wrapped() {
    assert_eq!(
        credits::plain("Fabián *DAFX* `x` [link](http://a)"),
        "FABIAN DAFX X LINK"
    );
    assert_eq!(credits::plain("Öörni — “hi” ≈"), "OORNI - \"HI\" ?");
    assert_eq!(
        credits::wrap("aaaa bbbb cccc dddd eeee ffff gggg"),
        ["aaaa bbbb cccc dddd eeee", "ffff gggg"]
    );
    assert_eq!(
        credits::wrap(&"x".repeat(30)),
        ["x".repeat(26), "x".repeat(4)]
    );
}
