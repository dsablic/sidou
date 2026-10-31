// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::credits::{self, Line, Page};

use crate::c_utf8;

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

pub(crate) fn compiled_pages() -> &'static [credits_page] {
    unsafe { std::slice::from_raw_parts(credits_pages.as_ptr(), credits_page_count as usize) }
}

fn line(text: &str, name: bool) -> Line {
    Line {
        text: text.into(),
        name,
    }
}

#[test]
fn the_compiled_table_is_the_file() {
    let want = credits::load();
    let pages = compiled_pages();
    assert_eq!(pages.len(), want.len());
    for (got, want) in pages.iter().zip(&want) {
        assert_eq!(c_utf8(got.heading), want.heading);
        let lines: Vec<Line> = got
            .line
            .iter()
            .take_while(|p| !p.is_null())
            .enumerate()
            .map(|(i, &p)| Line {
                text: c_utf8(p),
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
    for (k, p) in pages.iter().enumerate() {
        println!("page {}: {}", k + 1, p.heading);
        for l in &p.lines {
            println!("  {} {}", if l.name { '*' } else { ' ' }, l.text);
        }
    }
    for p in &pages {
        assert!(!p.lines.is_empty() && p.lines.len() <= CREDITS_LINES as usize);
        for l in std::iter::once(&p.heading).chain(p.lines.iter().map(|l| &l.text)) {
            assert!(l.len() <= CREDITS_COLS as usize, "{l:?} is too long");
            assert!(l.chars().all(credits::in_font), "{l:?} is outside the font");
            assert!(!l.contains('?'), "{l:?} lost a character");
        }
    }
}

fn bullets(md: &str) -> Vec<(String, String)> {
    let mut items: Vec<String> = Vec::new();
    for line in md.lines() {
        if let Some(b) = line.strip_prefix("- ") {
            items.push(b.to_string());
        } else if line.starts_with("  ") && !items.is_empty() && !line.trim().is_empty() {
            let last = items.last_mut().unwrap();
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    items
        .iter()
        .map(|b| {
            let (who, what) = match b.find("**:") {
                Some(i) => (&b[..i + 2], &b[i + 3..]),
                None => {
                    let end = b[2..].find("**").map_or(0, |i| i + 4);
                    (&b[..end], &b[end..])
                }
            };
            (credits::plain(who), credits::plain(what))
        })
        .collect()
}

fn name_parts(text: &str) -> Vec<String> {
    text.replace(" AND ", ", ")
        .replace("; ", ", ")
        .replace(" / ", ", ")
        .split(", ")
        .map(|p| p.trim().trim_start_matches("AND ").trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[test]
fn everyone_in_the_file_is_on_a_page() {
    let md = std::fs::read_to_string(credits::source_path()).unwrap();
    let pages = credits::load();
    let join = |name_only: bool| {
        pages
            .iter()
            .flat_map(|p| p.lines.iter())
            .filter(|l| !name_only || l.name)
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (all, names) = (join(false), join(true));
    let found = bullets(&md);
    assert!(found.len() >= 15, "{} bullets in CREDITS.md", found.len());
    for (who, what) in &found {
        assert!(!who.is_empty(), "a bullet names nobody: {what}");
        for part in name_parts(who) {
            assert!(
                names.contains(&part),
                "{part:?} is not shown as a name: {names}"
            );
            let whole = pages
                .iter()
                .flat_map(|p| p.lines.iter())
                .any(|l| l.name && l.text.contains(&part));
            assert!(part.len() > credits::COLS || whole, "{part:?} is cut");
        }
        for part in name_parts(what) {
            assert!(all.contains(&part), "{part:?} is missing: {all}");
        }
    }
    for who in [
        "JULIAN PARKER",
        "EFFLAM LE BIVIC",
        "STEFAN BILBAO",
        "JATIN CHOWDHURY",
    ] {
        assert!(names.contains(who), "{who}");
    }
    for who in ["LEANDRO NINI", "AROUM"] {
        assert!(all.contains(who), "{who}");
    }
}

#[test]
fn a_bold_name_without_a_colon_is_who_is_credited() {
    let pages = credits::parse("## S\n- **M-VAVE** for the firmware\n- **HVSC** team\n");
    assert_eq!(
        pages[0].lines,
        [
            line("M-VAVE", true),
            line("FOR THE FIRMWARE", false),
            line("HVSC", true),
            line("TEAM", false),
        ]
    );
}

#[test]
fn a_block_longer_than_a_page_starts_one_and_is_marked_where_it_goes_on() {
    let names = "Aaaaaa Aaaaaaa, Bbbbbb Bbbbbbbbb, Ccccc Cccc, Ddddddd Ddddd, Eeeeee Eeeeeeee, \
                 Fffff Ffffff";
    let md = format!(
        "## Heading\n- **Short**: one\n- **{names}**: one two three four five six seven \
         eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen \
         eighteen nineteen twenty\n- **After**: x\n"
    );
    let pages = credits::parse(&md);
    let heads: Vec<&str> = pages.iter().map(|p| p.heading.as_str()).collect();
    assert_eq!(heads, ["HEADING", "HEADING", "HEADING (CONT.)", "HEADING"]);
    assert_eq!(
        pages[0].lines.len(),
        2,
        "the long bullet did not start a page"
    );
    assert!(pages[1].lines.iter().all(|l| l.name), "{:?}", pages[1]);
    assert!(pages[2].lines.iter().all(|l| !l.name), "{:?}", pages[2]);
    assert_eq!(pages[3].lines[0].text, "AFTER");
    for p in &pages {
        assert!(p.lines.len() <= credits::LINES);
    }
}

#[test]
fn no_credits_page_starts_with_the_tail_of_another_and_continued_pages_are_marked() {
    for p in credits::load() {
        if p.heading.ends_with("(CONT.)") {
            assert!(p.lines.iter().all(|l| !l.name), "{p:?}");
        }
        let first = &p.lines[0].text;
        assert!(
            p.lines[0].name || p.heading.ends_with("(CONT.)") || !first.starts_with("FUZZ"),
            "{p:?}"
        );
    }
}

#[test]
fn bullets_split_into_who_and_what() {
    let pages = credits::parse(
        "<!-- SPDX -->\n# Credits\n\nIntro text.\n\n## Things\n\
         - **reSID**: Dag Lem\n- **Long**: one two\n  three\n- plain bullet\n",
    );
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
    assert_eq!(
        credits::wrap_names("JULIAN PARKER, VADIM ZAVALISHIN, EFFLAM LE BIVIC AND VESA VALIMAKI"),
        [
            "JULIAN PARKER,",
            "VADIM ZAVALISHIN,",
            "EFFLAM LE BIVIC AND",
            "VESA VALIMAKI"
        ]
    );
    assert_eq!(
        credits::wrap_names("EMILIE GILLET (MUTABLE INSTRUMENTS)"),
        ["EMILIE GILLET", "(MUTABLE INSTRUMENTS)"]
    );
    assert!(credits::is_name_list(
        "ANTTI S. LANKILA, LEANDRO NINI AND CONTRIBUTORS"
    ));
    assert!(!credits::is_name_list(
        "LEO KUROSHITA / HUGELTON INSTRUMENTS, AND THE MANY FORKS BUILT ON IT"
    ));
}
