// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The About cracktro's credits: `app/credits.c`, generated from `CREDITS.md`.
//!
//! Each `## Section` becomes one or more pages headed by its title. A bullet
//! `- **Who**: what` gives "who" lines (shown brighter) and "what" lines; a paragraph or a
//! bullet without a colon gives "what" lines only. The text before the first section goes
//! under the heading "CREDITS". Lines wrap at `CREDITS_COLS` (app/credits.h) in upper case;
//! accented Latin letters lose their accents and any other character outside the font's
//! ASCII becomes '?'. A bullet stays on one page unless it is longer than a page.

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

/// A block of the file: a bullet or a paragraph, under a heading.
struct Block {
    heading: String,
    lines: Vec<Line>,
}

fn block(heading: &str, md: &str, bullet: bool) -> Block {
    let mut lines = Vec::new();
    let split = if bullet && md.trim_start().starts_with("**") {
        md.find(": ").or_else(|| md.find("**:"))
    } else {
        None
    };
    let (who, what) = match split {
        Some(i) => {
            let (a, b) = md.split_at(i);
            (Some(a), b.trim_start_matches(['*', ':', ' ']))
        }
        None => (None, md),
    };
    if let Some(who) = who {
        lines.extend(
            wrap(&plain(who))
                .into_iter()
                .map(|text| Line { text, name: true }),
        );
    }
    lines.extend(
        wrap(&plain(what))
            .into_iter()
            .map(|text| Line { text, name: false }),
    );
    Block {
        heading: heading.to_string(),
        lines,
    }
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
    for b in blocks {
        if b.lines.is_empty() {
            continue;
        }
        let fits = pages
            .last()
            .is_some_and(|p| p.heading == b.heading && p.lines.len() + b.lines.len() <= LINES);
        if fits {
            pages.last_mut().unwrap().lines.extend(b.lines);
            continue;
        }
        for chunk in b.lines.chunks(LINES) {
            pages.push(Page {
                heading: b.heading.clone(),
                lines: chunk.to_vec(),
            });
        }
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
