// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's 8×8 screen font: the glyph sheet `assets/font/sidou-8x8.txt` (the source, its
//! format described at its top) and the C it generates, `app/font.c` and `app/font.h`.
//! `tests/app/font.rs` keeps the generated files in step with the sheet.

use std::fmt::Write as _;
use std::path::PathBuf;

/// Every cell value 0x00..0xFF has a glyph (blank unless the sheet draws one).
pub const GLYPHS: usize = 256;

/// The glyphs the firmware keeps: codes `0..=LAST` (the codes above are blank and never drawn;
/// `font_glyph` in `font.h`).
pub const LAST: usize = 0xDC;

/// The firmware's glyph for `code`, as compiled (`font_glyphs`); blank for a code above
/// `LAST`, which the firmware never draws.
pub fn compiled_glyph(code: u8) -> [u8; 8] {
    let rows = unsafe { *std::ptr::addr_of!(sidou_sys::font_glyphs) };
    rows.get(usize::from(code)).copied().unwrap_or([0; 8])
}

/// One sheet entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub code: u8,
    /// The character (`'A'`) or the NAME (`Q0`), as the sheet writes it.
    pub label: String,
    /// Set for a NAME: `FONT_<NAME>` in `font.h`.
    pub name: Option<String>,
    /// For "= <other code>": the glyph it reuses.
    pub alias_of: Option<u8>,
}

/// The parsed sheet: 256 glyphs of 8 rows, top row first, bit 7 the leftmost pixel.
#[derive(Clone, Debug)]
pub struct Font {
    pub glyphs: [[u8; 8]; GLYPHS],
    pub entries: Vec<Entry>,
}

impl Font {
    /// Whether pixel (x, y) of glyph `code` is set.
    pub fn pixel(&self, code: u8, x: usize, y: usize) -> bool {
        self.glyphs[usize::from(code)][y] & (0x80 >> x) != 0
    }

    /// The `FONT_<NAME>` value for `name`.
    pub fn code(&self, name: &str) -> u8 {
        self.entries
            .iter()
            .find(|e| e.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no glyph {name} in the font"))
            .code
    }
}

/// The repository root (this crate is `host/crates/sidou-tests`).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn sheet_path() -> PathBuf {
    repo_root().join("assets/font/sidou-8x8.txt")
}

/// Reads and parses the sheet.
pub fn load() -> Font {
    let path = sheet_path();
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn is_comment(line: &str) -> bool {
    line.is_empty() || line == "#" || line.starts_with("# ")
}

fn parse_code(s: &str) -> Result<u8, String> {
    let hex = s
        .strip_prefix("0x")
        .ok_or_else(|| format!("code {s:?} is not 0x.."))?;
    u8::from_str_radix(hex, 16).map_err(|e| format!("code {s:?}: {e}"))
}

fn parse_label(s: &str) -> Result<(String, Option<String>), String> {
    let quoted = s.len() == 3 && s.starts_with('\'') && s.ends_with('\'');
    if quoted {
        return Ok((s.to_owned(), None));
    }
    let ok = !s.is_empty()
        && s.starts_with(|c: char| c.is_ascii_uppercase())
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok((s.to_owned(), Some(s.to_owned())))
    } else {
        Err(format!("name {s:?} is neither 'c' nor a NAME"))
    }
}

/// Parses a sheet; errors name the line.
pub fn parse(text: &str) -> Result<Font, String> {
    let mut font = Font {
        glyphs: [[0; 8]; GLYPHS],
        entries: Vec::new(),
    };
    let mut lines = text.lines().enumerate().peekable();
    let mut last: Option<u8> = None;
    while let Some((n, raw)) = lines.next() {
        let line = raw.trim_end();
        if is_comment(line) {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", n + 1);
        let (head, alias) = match line.split_once(" = ") {
            Some((h, a)) => (h, Some(parse_code(a.trim()).map_err(at)?)),
            None => (line, None),
        };
        let (code, label) = head
            .split_once(' ')
            .ok_or_else(|| at(format!("expected \"<code> <name>\", got {line:?}")))?;
        let code = parse_code(code).map_err(at)?;
        if last.is_some_and(|l| code <= l) {
            return Err(at(format!("code {code:#04x} is out of order")));
        }
        last = Some(code);
        let (label, name) = parse_label(label).map_err(at)?;
        if let Some(name) = &name {
            if font.entries.iter().any(|e| e.name.as_ref() == Some(name)) {
                return Err(at(format!("{name} is defined twice")));
            }
        }
        if let Some(a) = alias {
            if !font
                .entries
                .iter()
                .any(|e| e.code == a && e.alias_of.is_none())
            {
                return Err(at(format!("{a:#04x} is not a drawn glyph above")));
            }
            font.glyphs[usize::from(code)] = font.glyphs[usize::from(a)];
        } else {
            let mut rows = [0u8; 8];
            for row in &mut rows {
                let (rn, r) = lines
                    .next()
                    .ok_or_else(|| at("the glyph has fewer than 8 rows".into()))?;
                let r = r.trim_end();
                if r.len() != 8 || !r.chars().all(|c| c == '#' || c == '.') {
                    return Err(format!(
                        "line {}: a row is 8 of '#' and '.', got {r:?}",
                        rn + 1
                    ));
                }
                *row = r
                    .chars()
                    .enumerate()
                    .filter(|&(_, c)| c == '#')
                    .fold(0, |b, (x, _)| b | 0x80 >> x);
            }
            font.glyphs[usize::from(code)] = rows;
        }
        font.entries.push(Entry {
            code,
            label,
            name,
            alias_of: alias,
        });
    }
    Ok(font)
}

/// Names the generator; the first line of each generated file's comment, within 100 columns.
const GENERATED: &str = "Generated by host/crates/sidou-tests/src/font.rs from \
                         assets/font/sidou-8x8.txt;\n * do not edit.";

// The generated files' own licence headers; reuse must not read them as this file's.
// REUSE-IgnoreStart
const SPDX: &str = "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
                    // SPDX-License-Identifier: GPL-3.0-or-later\n";
// REUSE-IgnoreEnd

/// `app/font.h`.
pub fn c_header(font: &Font) -> String {
    let mut s = String::from(SPDX);
    let _ = write!(
        s,
        "/* {GENERATED}\n \
         * SIDOU's 8x8 PETSCII-style screen font (docs/art/README.md): glyphs of 8 rows, top\n \
         * row first, bit 7 the leftmost pixel. ASCII 0x20..0x7E (lower case drawn as upper case)\n \
         * and the graphics below; codes without a name or a character are blank. Only the codes\n \
         * 0..FONT_LAST are in flash: nothing draws a code above them. */\n\
         #ifndef SIDOU_FONT_H\n\
         #define SIDOU_FONT_H\n\n\
         #include <stdint.h>\n\n\
         #ifdef __cplusplus\n\
         extern \"C\" {{\n\
         #endif\n\n"
    );
    let _ = writeln!(s, "#define FONT_LAST 0x{LAST:02X}u\n");
    for e in font.entries.iter().filter(|e| e.name.is_some()) {
        let _ = writeln!(
            s,
            "#define FONT_{} 0x{:02X}u",
            e.name.as_ref().unwrap(),
            e.code
        );
    }
    s.push_str(
        "\n/* In flash (.rodata): read by the renderer only, 4-byte aligned (grid.c tests a glyph for blank\n\
         \x20* as two words). */\n\
         extern const uint8_t font_glyphs[FONT_LAST + 1u][8] __attribute__((aligned(4)));\n\n\
         /* A code's 8 rows. A code above FONT_LAST reads past the table: the host\n\
         \x20* build (SIDOU_FONT_CHECK) aborts on one, which is how the tests prove nothing draws it. */\n\
         #ifdef SIDOU_FONT_CHECK\n\
         const uint8_t* font_glyph_checked(uint32_t code);\n\
         #define font_glyph(code) font_glyph_checked((uint8_t)(code))\n\
         #else\n\
         #define font_glyph(code) (font_glyphs[(uint8_t)(code)])\n\
         #endif\n\n\
         #ifdef __cplusplus\n\
         }\n\
         #endif\n\n\
         #endif\n",
    );
    s
}

/// A label safe inside a C comment.
fn comment_label(label: &str) -> String {
    label.replace("*/", "* /").replace("/*", "/ *")
}

/// `app/font.c`.
pub fn c_source(font: &Font) -> String {
    let mut s = String::from(SPDX);
    let _ = write!(
        s,
        "/* {GENERATED} */\n\
         #include \"font.h\"\n\n\
         #ifdef SIDOU_FONT_CHECK\n\
         #include <stdio.h>\n\
         #include <stdlib.h>\n\
         #endif\n\n\
         const uint8_t font_glyphs[FONT_LAST + 1u][8] __attribute__((aligned(4))) = {{\n"
    );
    for (code, rows) in font.glyphs.iter().enumerate().take(LAST + 1) {
        let bytes: Vec<String> = rows.iter().map(|b| format!("0x{b:02x}")).collect();
        let label = font
            .entries
            .iter()
            .find(|e| usize::from(e.code) == code)
            .map(|e| format!(" {}", comment_label(&e.label)))
            .unwrap_or_default();
        let _ = writeln!(s, "    {{{}}}, /* 0x{code:02X}{label} */", bytes.join(", "));
    }
    s.push_str(
        "};\n\n\
         #ifdef SIDOU_FONT_CHECK\n\
         const uint8_t* font_glyph_checked(uint32_t code) {\n    \
         if (code > FONT_LAST) {\n        \
         fprintf(stderr, \"font: code 0x%02X is not in flash\\n\", (unsigned)code);\n        \
         abort();\n    \
         }\n    \
         return font_glyphs[code];\n\
         }\n\
         #endif\n",
    );
    s
}

/// Where the generated files live.
pub fn generated_paths() -> (PathBuf, PathBuf) {
    let app = repo_root().join("app");
    (app.join("font.h"), app.join("font.c"))
}

/// Writes `font.h` and `font.c` if `update`, else returns the stale ones.
pub fn check_or_write(font: &Font, update: bool) -> Vec<PathBuf> {
    let (h, c) = generated_paths();
    let mut stale = Vec::new();
    for (path, want) in [(h, c_header(font)), (c, c_source(font))] {
        if update {
            std::fs::write(&path, &want)
                .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(want.as_str()) {
            stale.push(path);
        }
    }
    stale
}
