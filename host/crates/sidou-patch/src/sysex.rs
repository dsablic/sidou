// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub const START: u8 = 0xF0;
pub const END: u8 = 0xF7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Framing {
    NotSysex { at: usize },
    Unterminated { at: usize },
}

pub fn messages(bytes: &[u8]) -> impl Iterator<Item = Result<(usize, &[u8]), Framing>> {
    let mut at = 0;
    std::iter::from_fn(move || {
        if at >= bytes.len() {
            return None;
        }
        let start = at;
        if bytes[start] != START {
            at = bytes.len();
            return Some(Err(Framing::NotSysex { at: start }));
        }
        let Some(length) = bytes[start..].iter().position(|&b| b == END) else {
            at = bytes.len();
            return Some(Err(Framing::Unterminated { at: start }));
        };
        at = start + length + 1;
        Some(Ok((start, &bytes[start + 1..start + length])))
    })
}

pub fn ascii_name(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7F).contains(&b) {
                b as char
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}
