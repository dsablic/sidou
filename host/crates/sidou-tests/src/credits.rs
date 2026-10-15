// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The About cracktro's credits: `app/credits.c`, generated from `CREDITS.md`.
//!
//! Each `## Section` becomes one or more pages headed by its title. A bullet
//! `- **Who**: what` gives "who" lines (shown brighter) and "what" lines; so does a bullet
//! that starts in bold without a colon (`- **Who** what`); a paragraph or any other bullet
//! gives "what" lines only. The text before the first section goes under the heading
//! "CREDITS". Names wrap between names, never inside one that fits a line ("who", and a
//! "what" that is a list of names); other text wraps at any space. Lines wrap at `CREDITS_COLS` (app/credits.h) in upper case; accented Latin
//! letters lose their accents and any other character outside the font's ASCII becomes '?'.
//!
//! A bullet stays on one page when it fits one, after the bullets before it if they leave
//! room. One longer than a page starts a page of its own, and goes on over as many pages as it
//! needs, each headed `<SECTION> (CONT.)`, cut where who ends and what begins rather than
//! inside a sentence when each fits a page. Nothing follows it on its last page: the credits
//! are attribution, and a bullet's tail never runs into the next one.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::font::repo_root;

/// app/credits.h
pub const COLS: usize = 26;
pub const LINES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    /// Names who is credited.
    pub name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub heading: String,
    pub lines: Vec<Line>,
}

pub fn source_path() -> PathBuf {
    repo_root().join("CREDITS.md")
}

pub fn generated_path() -> PathBuf {
    repo_root().join("app/credits.c")
}

/// The characters the font draws as themselves (upper case ASCII and punctuation).
pub fn in_font(c: char) -> bool {
    (' '..='_').contains(&c)
}

/// One character in the font's terms: upper case, accents dropped, quotes and dashes made
/// ASCII; '?' for the rest.
fn fold(c: char, out: &mut String) {
    let base = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "A",
        'ç' | 'Ç' => "C",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "E",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "I",
        'ñ' | 'Ñ' => "N",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "O",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "U",
        'ý' | 'ÿ' | 'Ý' => "Y",
        'ß' => "SS",
        '“' | '”' | '„' => "\"",
        '‘' | '’' => "'",
        '–' | '—' => "-",
        '…' => "...",
        _ => "",
    };
    if !base.is_empty() {
        out.push_str(base);
        return;
    }
    let up = c.to_ascii_uppercase();
    out.push(if in_font(up) { up } else { '?' });
}

/// Markdown inline text to the font: emphasis and code marks dropped, links to their text.
pub fn plain(md: &str) -> String {
    let mut s = String::new();
    let mut chars = md.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '`' | '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for n in chars.by_ref() {
                        if n == ')' {
                            break;
                        }
                    }
                }
            }
            _ => fold(c, &mut s),
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Words into lines of at most `COLS` characters; a longer word is cut.
pub fn wrap(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.len() > COLS {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            let rest = word.split_off(COLS);
            lines.push(word);
            word = rest;
        }
        if cur.is_empty() {
            cur = word;
        } else if cur.len() + 1 + word.len() <= COLS {
            cur.push(' ');
            cur.push_str(&word);
        } else {
            lines.push(std::mem::replace(&mut cur, word));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// `text` split after each `sep`, which stays (trimmed) at the end of the part before it.
fn split_after(text: &str, sep: &str) -> Vec<String> {
    let parts: Vec<&str> = text.split(sep).collect();
    let last = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i < last {
                format!("{p}{}", sep.trim_end())
            } else {
                (*p).to_string()
            }
        })
        .collect()
}

/// A list of names in parts never to be cut: after each ", " and " AND ", and a part longer
/// than a line before its " (" when both halves fit one ("EMILIE GILLET" and "(MUTABLE
/// INSTRUMENTS)").
fn name_units(text: &str) -> Vec<String> {
    let mut units = Vec::new();
    for part in split_after(text, ", ") {
        for p in split_after(&part, " AND ") {
            let p = p.trim().to_string();
            match p.find(" (") {
                Some(i) if p.len() > COLS && i <= COLS && p.len() - i - 1 <= COLS => {
                    units.push(p[..i].to_string());
                    units.push(p[i + 1..].to_string());
                }
                _ if !p.is_empty() => units.push(p),
                _ => {}
            }
        }
    }
    units
}

/// Whether text is a list of names (each part of it at most 4 words), not a sentence.
pub fn is_name_list(text: &str) -> bool {
    name_units(text)
        .iter()
        .all(|u| u.split_whitespace().count() <= 4)
}

/// A list of names into lines of at most `COLS` characters, each name whole on one line when
/// it fits one (`wrap` cuts at any space: "VADIM / ZAVALISHIN").
pub fn wrap_names(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for u in name_units(text) {
        if u.len() > COLS {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.extend(wrap(&u));
        } else if cur.is_empty() {
            cur = u;
        } else if cur.len() + 1 + u.len() <= COLS {
            cur.push(' ');
            cur.push_str(&u);
        } else {
            lines.push(std::mem::replace(&mut cur, u));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// A block of the file: a bullet or a paragraph, under a heading; who is credited, then what
/// for.
struct Block {
    heading: String,
    who: Vec<Line>,
    what: Vec<Line>,
}

impl Block {
    fn len(&self) -> usize {
        self.who.len() + self.what.len()
    }

    fn lines(&self) -> impl Iterator<Item = &Line> {
        self.who.iter().chain(&self.what)
    }
}

/// A bullet's who and what: `**Who**: what` splits at the colon after the bold, and a bullet
/// that starts in bold without one at the bold's end (`**M-VAVE** for publishing ...`).
fn who_and_what(md: &str) -> (Option<&str>, &str) {
    let md = md.trim_start();
    if !md.starts_with("**") {
        return (None, md);
    }
    if let Some(i) = md.find("**:") {
        return (Some(&md[..i + 2]), &md[i + 3..]);
    }
    match md[2..].find("**") {
        Some(i) => (Some(&md[..i + 4]), &md[i + 4..]),
        None => (None, md),
    }
}

fn block(heading: &str, md: &str, bullet: bool) -> Block {
    let (who, what) = if bullet { who_and_what(md) } else { (None, md) };
    // Who is credited, and a list of names after the colon, wrap between names.
    let lines = |text: &str, name: bool| {
        let text = plain(text);
        let wrapped = if name || (bullet && is_name_list(&text)) {
            wrap_names(&text)
        } else {
            wrap(&text)
        };
        wrapped
            .into_iter()
            .map(|text| Line { text, name })
            .collect::<Vec<_>>()
    };
    Block {
        heading: heading.to_string(),
        who: who.map(|w| lines(w, true)).unwrap_or_default(),
        what: lines(what, false),
    }
}

/// The heading of the pages a block goes on over.
pub fn continued(heading: &str) -> String {
    let h = format!("{heading} (CONT.)");
    if h.len() <= COLS {
        h
    } else {
        String::from("(CONT.)")
    }
}

/// A block longer than a page in page-sized parts: who and what apart when each fits a page,
/// else page-sized cuts of the whole.
fn parts(b: &Block) -> Vec<Vec<Line>> {
    if b.who.len() <= LINES && b.what.len() <= LINES {
        return [&b.who, &b.what]
            .into_iter()
            .filter(|p| !p.is_empty())
            .map(|p| p.to_vec())
            .collect();
    }
    let all: Vec<Line> = b.lines().cloned().collect();
    all.chunks(LINES).map(<[Line]>::to_vec).collect()
}

/// CREDITS.md into pages.
pub fn parse(md: &str) -> Vec<Page> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut heading = String::from("CREDITS");
    let mut cur: Option<(String, bool)> = None;
    let flush = |cur: &mut Option<(String, bool)>, heading: &str, blocks: &mut Vec<Block>| {
        if let Some((text, bullet)) = cur.take() {
            blocks.push(block(heading, &text, bullet));
        }
    };
    let mut in_comment = false;
    for raw in md.lines() {
        let line = raw.trim_end();
        if in_comment || line.trim_start().starts_with("<!--") {
            in_comment = !line.contains("-->");
            continue;
        }
        if let Some(h) = line.strip_prefix("## ") {
            flush(&mut cur, &heading, &mut blocks);
            heading = plain(h);
        } else if line.starts_with("# ") {
            flush(&mut cur, &heading, &mut blocks);
        } else if let Some(b) = line.strip_prefix("- ") {
            flush(&mut cur, &heading, &mut blocks);
            cur = Some((b.to_string(), true));
        } else if line.trim().is_empty() {
            flush(&mut cur, &heading, &mut blocks);
        } else if let Some((text, _)) = cur.as_mut() {
            text.push(' ');
            text.push_str(line.trim());
        } else {
            cur = Some((line.trim().to_string(), false));
        }
    }
    flush(&mut cur, &heading, &mut blocks);

    let mut pages: Vec<Page> = Vec::new();
    // The last page ends a split block: nothing goes after it.
    let mut sealed = false;
    for b in blocks {
        if b.len() == 0 {
            continue;
        }
        if b.len() <= LINES {
            let fits = !sealed
                && pages
                    .last()
                    .is_some_and(|p| p.heading == b.heading && p.lines.len() + b.len() <= LINES);
            if fits {
                pages.last_mut().unwrap().lines.extend(b.lines().cloned());
            } else {
                pages.push(Page {
                    heading: b.heading.clone(),
                    lines: b.lines().cloned().collect(),
                });
            }
            sealed = false;
            continue;
        }
        for (i, part) in parts(&b).into_iter().enumerate() {
            pages.push(Page {
                heading: if i == 0 {
                    b.heading.clone()
                } else {
                    continued(&b.heading)
                },
                lines: part,
            });
        }
        sealed = true;
    }
    pages
}

pub fn load() -> Vec<Page> {
    let path = source_path();
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse(&text)
}

fn c_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

// REUSE-IgnoreStart
const HEADER: &str = "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
                      // SPDX-License-Identifier: GPL-3.0-or-later\n\
                      /* Generated by host/crates/sidou-tests/src/credits.rs from CREDITS.md; \
                      do not edit. */\n";
// REUSE-IgnoreEnd

/// `app/credits.c`.
pub fn c_source(pages: &[Page]) -> String {
    let mut s = String::from(HEADER);
    s.push_str("#include \"credits.h\"\n\n#include <stddef.h>\n\n");
    s.push_str("const credits_page credits_pages[] = {\n");
    for p in pages {
        let names = p
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.name)
            .fold(0u8, |m, (i, _)| m | 1 << i);
        let _ = writeln!(s, "    {{{},", c_string(&p.heading));
        s.push_str("     {");
        for (i, l) in p.lines.iter().enumerate() {
            if i > 0 {
                s.push_str(",\n      ");
            }
            s.push_str(&c_string(&l.text));
        }
        for _ in p.lines.len()..LINES {
            s.push_str(", NULL");
        }
        let _ = writeln!(s, "}},\n     0x{names:02X}u}},");
    }
    s.push_str("};\n\n");
    let _ = writeln!(
        s,
        "const uint32_t credits_page_count = sizeof credits_pages / sizeof credits_pages[0];"
    );
    s
}

/// Writes `app/credits.c` if `update`, else returns whether it is stale.
pub fn check_or_write(pages: &[Page], update: bool) -> bool {
    let path = generated_path();
    let want = c_source(pages);
    if update {
        std::fs::write(&path, &want).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        return false;
    }
    std::fs::read_to_string(&path).ok().as_deref() != Some(want.as_str())
}
