// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub const MAGIC: [u8; 5] = [0x7d, b'F', b'M', b'1', b'L'];
pub const VERSION: u8 = 1;
pub const PAYLOAD_MAX: usize = 48;

pub const START: u8 = 0x01;
pub const HEARTBEAT: u8 = 0x02;
pub const STOP: u8 = 0x03;
pub const KEYFRAME: u8 = 0x04;
pub const FONT: u8 = 0x05;
pub const PALETTE: u8 = 0x06;
pub const PANEL: u8 = 0x07;
pub const HELLO: u8 = 0x41;
pub const BEAT: u8 = 0x42;
pub const STOPPED: u8 = 0x43;
pub const COLOURS: u8 = 0x44;
pub const GLYPHS: u8 = 0x45;
pub const CELLS: u8 = 0x46;
pub const PIXELS: u8 = 0x47;

pub const OVERLAY: u8 = 0x10;

pub fn pack(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + payload.len().div_ceil(7));
    for group in payload.chunks(7) {
        out.push(
            group
                .iter()
                .enumerate()
                .fold(0, |m, (k, b)| m | (b >> 7) << k),
        );
        out.extend(group.iter().map(|b| b & 0x7f));
    }
    out
}

pub fn unpack(packed: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(packed.len());
    for group in packed.chunks(8) {
        let (mask, low) = group.split_first()?;
        if low.is_empty() || mask >> low.len() != 0 || group.iter().any(|b| b & 0x80 != 0) {
            return None;
        }
        out.extend(
            low.iter()
                .enumerate()
                .map(|(k, b)| b | (mask >> k & 1) << 7),
        );
    }
    Some(out)
}

pub fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0xf0];
    out.extend(MAGIC);
    out.push(cmd);
    out.extend(pack(payload));
    let sum = out[6..].iter().fold(0u8, |s, b| s.wrapping_add(*b));
    out.push(sum.wrapping_neg() & 0x7f);
    out.push(0xf7);
    out
}

pub fn parse(f: &[u8]) -> Option<(u8, Vec<u8>)> {
    if f.len() < 9 || f[0] != 0xf0 || f[1..6] != MAGIC || *f.last()? != 0xf7 {
        return None;
    }
    let body = &f[6..f.len() - 1];
    if body.iter().fold(0u8, |s, b| s.wrapping_add(*b)) & 0x7f != 0 {
        return None;
    }
    Some((body[0], unpack(&body[1..body.len() - 1])?))
}

pub fn start() -> Vec<u8> {
    frame(START, &[VERSION])
}
pub fn heartbeat() -> Vec<u8> {
    frame(HEARTBEAT, &[])
}
pub fn stop() -> Vec<u8> {
    frame(STOP, &[])
}
pub fn keyframe() -> Vec<u8> {
    frame(KEYFRAME, &[])
}
pub fn font() -> Vec<u8> {
    frame(FONT, &[])
}
pub fn palette() -> Vec<u8> {
    frame(PALETTE, &[])
}
pub fn panel(held: u16, pressed: u16, steps: &[i8]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend(held.to_le_bytes());
    p.extend(pressed.to_le_bytes());
    p.extend(steps.iter().map(|&s| s as u8));
    frame(PANEL, &p)
}

#[derive(Default)]
pub struct Splitter {
    frame: Vec<u8>,
    open: bool,
}

impl Splitter {
    pub fn push(&mut self, b: u8, out: &mut Vec<Vec<u8>>) {
        if b >= 0xf8 {
            return;
        }
        if b == 0xf0 {
            self.frame.clear();
            self.open = true;
        } else if !self.open {
            return;
        } else if b & 0x80 != 0 && b != 0xf7 {
            self.open = false;
            return;
        }
        self.frame.push(b);
        if b == 0xf7 {
            self.open = false;
            out.push(std::mem::take(&mut self.frame));
        }
    }

    pub fn packets(&mut self, packets: &[u8], out: &mut Vec<Vec<u8>>) {
        for p in packets.as_chunks::<4>().0 {
            let n = match p[0] & 15 {
                5 | 0xf => 1,
                2 | 6 | 0xc | 0xd => 2,
                3 | 4 | 7..=0xb | 0xe => 3,
                _ => 0,
            };
            for &b in &p[1..=n] {
                self.push(b, out);
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hello {
    pub version: u8,
    pub features: u8,
    pub cols: usize,
    pub rows: usize,
    pub cell_w: usize,
    pub cell_h: usize,
    pub colours: usize,
    pub font_first: u8,
    pub font_last: u8,
    pub piece_first: u8,
    pub pieces: u8,
    pub timeout_ms: u32,
    pub buttons: u8,
    pub encoders: u8,
    pub font_crc: u32,
    pub palette_crc: u32,
    pub name: String,
}

impl Hello {
    pub fn parse(p: &[u8]) -> Option<Self> {
        if p.len() < 22 {
            return None;
        }
        let le = |i: usize| u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
        Some(Hello {
            version: p[0],
            features: p[1],
            cols: p[2].into(),
            rows: p[3].into(),
            cell_w: p[4].into(),
            cell_h: p[5].into(),
            colours: p[6].into(),
            font_first: p[7],
            font_last: p[8],
            piece_first: p[9],
            pieces: p[10],
            timeout_ms: u32::from(p[11]) * 100,
            buttons: p[12],
            encoders: p[13],
            font_crc: le(14),
            palette_crc: le(18),
            name: String::from_utf8_lossy(&p[22..]).into_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub ch: u8,
    pub attr: u8,
    pub x: u8,
    pub mask: [u8; 8],
    pub pixels: Option<[u8; 64]>,
}

pub struct Mirror {
    pub hello: Option<Hello>,
    pub cells: Vec<Cell>,
    pub font: Vec<[u8; 8]>,
    pub palette: [u16; 16],
    pub on: bool,
    pub rejected: usize,
    pub cell_bytes: usize,
}

impl Default for Mirror {
    fn default() -> Self {
        Mirror {
            hello: None,
            cells: Vec::new(),
            font: vec![[0; 8]; 256],
            palette: [0; 16],
            on: false,
            rejected: 0,
            cell_bytes: 0,
        }
    }
}

impl Mirror {
    pub fn cols(&self) -> usize {
        self.hello.as_ref().map_or(0, |h| h.cols)
    }

    pub fn rows(&self) -> usize {
        self.hello.as_ref().map_or(0, |h| h.rows)
    }

    pub fn take(&mut self, f: &[u8]) -> bool {
        let Some((cmd, p)) = parse(f) else {
            self.rejected += 1;
            return false;
        };
        self.apply(cmd, &p)
    }

    pub fn apply(&mut self, cmd: u8, p: &[u8]) -> bool {
        match cmd {
            HELLO => {
                let Some(h) = Hello::parse(p) else {
                    return false;
                };
                self.cells = vec![Cell::default(); h.cols * h.rows];
                self.hello = Some(h);
                self.on = true;
            }
            STOPPED => self.on = false,
            BEAT => {}
            COLOURS => {
                for (c, rgb) in p.as_chunks::<2>().0.iter().take(16).enumerate() {
                    self.palette[c] = u16::from_le_bytes([rgb[0], rgb[1]]);
                }
            }
            GLYPHS => {
                let Some((&first, rows)) = p.split_first() else {
                    return false;
                };
                for (k, g) in rows.as_chunks::<8>().0.iter().enumerate() {
                    if let Some(slot) = self.font.get_mut(usize::from(first) + k) {
                        slot.copy_from_slice(g);
                    }
                }
            }
            CELLS | PIXELS => return self.cells_frame(cmd, p),
            _ => return false,
        }
        true
    }

    fn cells_frame(&mut self, cmd: u8, p: &[u8]) -> bool {
        let (cols, rows) = (self.cols(), self.rows());
        if p.len() < 5 || usize::from(p[0]) >= rows {
            return false;
        }
        self.cell_bytes += p.len();
        let (row, mut col) = (usize::from(p[0]), usize::from(p[1]));
        let mut at = 2;
        while at < p.len() {
            if col >= cols || at + 3 > p.len() {
                return false;
            }
            let mut c = Cell {
                ch: p[at],
                attr: p[at + 1],
                x: p[at + 2],
                ..Cell::default()
            };
            at += 3;
            let piece = cmd == CELLS && self.is_piece(c.ch);
            if !piece && c.x & OVERLAY != 0 {
                c.mask.copy_from_slice(p.get(at..at + 8).unwrap_or(&[0; 8]));
                at += 8;
            }
            if cmd == PIXELS {
                let px = p.get(at..at + 32).unwrap_or(&[0; 32]);
                let mut all = [0u8; 64];
                for (k, b) in px.iter().enumerate() {
                    all[2 * k] = b >> 4;
                    all[2 * k + 1] = b & 15;
                }
                c.pixels = Some(all);
                at += 32;
            }
            self.cells[row * cols + col] = c;
            col += 1;
            if cmd == PIXELS {
                break;
            }
        }
        at == p.len()
    }

    fn is_piece(&self, ch: u8) -> bool {
        self.hello
            .as_ref()
            .is_some_and(|h| ch.wrapping_sub(h.piece_first) < h.pieces)
    }

    pub fn render(&self) -> Vec<u16> {
        let Some(h) = &self.hello else {
            return Vec::new();
        };
        let w = h.cols * 8;
        let mut fb = vec![0u16; w * h.rows * 8];
        for (i, c) in self.cells.iter().enumerate() {
            let (x0, y0) = (i % h.cols * 8, i / h.cols * 8);
            for (y, x, colour) in self.cell_pixels(c) {
                fb[(y0 + y) * w + x0 + x] = self.palette[usize::from(colour & 15)];
            }
        }
        fb
    }

    fn cell_pixels(&self, c: &Cell) -> impl Iterator<Item = (usize, usize, u8)> + '_ {
        let c = *c;
        let (fg, bg) = (c.attr & 15, c.attr >> 4);
        let piece = c.pixels.is_none() && self.is_piece(c.ch);
        let overlay = !piece && c.x & OVERLAY != 0;
        (0..64).map(move |k| {
            let (y, x) = (k / 8, k % 8);
            let bit = |g: &[u8; 8], y: usize, x: usize| g[y] >> (7 - x) & 1 != 0;
            let colour = if overlay && c.mask[y] >> (7 - x) & 1 != 0 {
                c.x & 15
            } else if piece {
                let p = c.ch - self.hello.as_ref().map_or(0, |h| h.piece_first);
                let (scale, q) = match p {
                    0..=3 => (2, usize::from(p)),
                    4..=12 => (3, usize::from(p) - 4),
                    _ => (1, 0),
                };
                let (gx, gy) = ((q % scale * 8 + x) / scale, (q / scale * 8 + y) / scale);
                if bit(&self.font[usize::from(c.x)], gy, gx) {
                    fg
                } else {
                    bg
                }
            } else if bit(&self.font[usize::from(c.ch)], y, x) {
                fg
            } else if let Some(px) = &c.pixels {
                px[k]
            } else {
                bg
            };
            (y, x, colour)
        })
    }
}
