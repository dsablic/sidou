// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::bitmap_codec;
use crate::font::repo_root;

pub const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0xff, 0xff, 0xff],
    [0x68, 0x37, 0x2b],
    [0x70, 0xa4, 0xb2],
    [0x6f, 0x3d, 0x86],
    [0x58, 0x8d, 0x43],
    [0x35, 0x28, 0x79],
    [0xb8, 0xc7, 0x6f],
    [0x6f, 0x4f, 0x25],
    [0x43, 0x39, 0x00],
    [0x9a, 0x67, 0x59],
    [0x44, 0x44, 0x44],
    [0x6c, 0x6c, 0x6c],
    [0x9a, 0xd2, 0x84],
    [0x6c, 0x5e, 0xb5],
    [0x95, 0x95, 0x95],
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    pub rows: Vec<u16>,
    pub data: Vec<u8>,
}

const RUN: usize = 16;

pub const SEEK: usize = 64;
pub const FORMAT: bitmap_codec::Format = bitmap_codec::Format {
    seek: SEEK,
    raw: false,
};

#[derive(Clone, Copy, Debug)]
pub struct Logo {
    pub name: &'static str,
    pub title: &'static str,
    pub symbol: &'static str,
    pub format: bitmap_codec::Format,
}

pub const LOGOS: [Logo; 2] = [
    Logo {
        name: "metal-chrome",
        title: "Metal Chrome",
        symbol: "logo_metal_chrome_bitmap",
        format: bitmap_codec::Format {
            seek: 256,
            raw: true,
        },
    },
    Logo {
        name: "cracktro-chrome",
        title: "Cracktro Chrome",
        symbol: "logo_cracktro_chrome_bitmap",
        format: bitmap_codec::Format {
            seek: 256,
            raw: false,
        },
    },
];

pub const DEFAULT: &str = "metal-chrome";

pub fn png_path(name: &str) -> PathBuf {
    repo_root().join(format!("assets/logo/{name}.png"))
}

pub fn generated_path() -> PathBuf {
    repo_root().join("app/logo_bitmap.c")
}

pub fn load(name: &str) -> Image {
    let path = png_path(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_png(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn parse_png(bytes: &[u8]) -> Result<Image, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .map_err(|e| format!("not a PNG: {e}"))?;
    let info = dec.info();
    let (w, h) = (info.width as usize, info.height as usize);
    if info.color_type != png::ColorType::Indexed {
        return Err(format!("{:?}, not an indexed PNG", info.color_type));
    }
    if w == 0 || h == 0 || w % 8 != 0 || h % 8 != 0 || w > 240 || h > 240 {
        return Err(format!(
            "{w} x {h}: not whole cells of 8 x 8 pixels within 240 x 240"
        ));
    }
    let depth = info.bit_depth as usize;
    let plte = info.palette.as_ref().ok_or("no PLTE chunk")?.to_vec();
    let trns = info.trns.as_ref().map(|t| t.to_vec()).unwrap_or_default();
    let mut buf = vec![0; dec.output_buffer_size().ok_or("PNG size")?];
    let frame = dec
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG data: {e}"))?;
    let stride = frame.line_size;
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let line = &buf[y * stride..(y + 1) * stride];
        for x in 0..w {
            let bit = x * depth;
            let entry = usize::from(line[bit / 8] >> (8 - depth - bit % 8)) & ((1 << depth) - 1);
            let rgb = plte
                .get(entry * 3..entry * 3 + 3)
                .ok_or(format!("({x}, {y}): entry {entry} is past the palette"))?;
            if trns.get(entry).is_some_and(|&a| a != 255) {
                return Err(format!("({x}, {y}): entry {entry} is transparent"));
            }
            let c = PALETTE.iter().position(|p| p == rgb).ok_or(format!(
                "({x}, {y}): {rgb:02x?} is not an art palette colour"
            ))?;
            px.push(c as u8);
        }
    }
    Ok(Image { w, h, px })
}

fn row_runs(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut x = 0;
    while x < row.len() {
        let c = row[x];
        let n = row[x..].iter().take(RUN).take_while(|&&p| p == c).count();
        out.push(((n - 1) << 4) as u8 | c);
        x += n;
    }
    out
}

pub fn encode(img: &Image) -> Encoded {
    let (mut rows, data, _) = encode_shared(&[(img, FORMAT)]);
    Encoded {
        rows: rows.remove(0),
        data,
    }
}

pub fn encode_shared(
    imgs: &[(&Image, bitmap_codec::Format)],
) -> (Vec<Vec<u16>>, Vec<u8>, Vec<usize>) {
    let rows: Vec<Vec<Vec<u8>>> = imgs
        .iter()
        .map(|(img, _)| img.px.chunks(img.w).map(row_runs).collect())
        .collect();
    bitmap_codec::encode_formats(
        &rows,
        &imgs.iter().map(|(_, format)| *format).collect::<Vec<_>>(),
    )
}

pub fn decode(enc: &Encoded, w: usize, h: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity(w * h);
    for &row in &enc.rows[..h] {
        let (mut at, _) = bitmap_codec::seek_row(&enc.data, row, w, 0);
        let mut n = 0;
        while n < w {
            let b = bitmap_codec::next(&enc.data, &mut at);
            let len = usize::from(b >> 4) + 1;
            px.extend(std::iter::repeat_n(b & 15, len));
            n += len;
        }
        assert_eq!(n, w, "a row's runs add up to its width");
    }
    px
}

// REUSE-IgnoreStart
pub(crate) const SPDX: &str = "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
                               // SPDX-License-Identifier: GPL-3.0-or-later\n";
// REUSE-IgnoreEnd

pub(crate) fn table<T: std::fmt::Display>(
    items: &[T],
    per: usize,
    fmt: impl Fn(&T) -> String,
) -> String {
    let mut s = String::new();
    for chunk in items.chunks(per) {
        let line: Vec<String> = chunk.iter().map(&fmt).collect();
        let _ = writeln!(s, "    {},", line.join(", "));
    }
    s
}

pub fn size(img: &Image, format: bitmap_codec::Format) -> (usize, usize) {
    let (rows, data, _) = encode_shared(&[(img, format)]);
    (data.len(), 2 * rows[0].len())
}

pub fn byte_run_size(img: &Image) -> usize {
    let rows: std::collections::HashSet<_> = img.px.chunks(img.w).map(row_runs).collect();
    rows.iter().map(Vec::len).sum::<usize>() + 2 * img.h
}

pub fn c_source() -> String {
    let imgs: Vec<Image> = LOGOS.iter().map(|l| load(l.name)).collect();
    let (rows, data, bases) = encode_shared(
        &imgs
            .iter()
            .zip(LOGOS)
            .map(|(img, l)| (img, l.format))
            .collect::<Vec<_>>(),
    );
    let mut s = String::from(SPDX);
    s += "/* Generated by host/crates/sidou-tests/src/logo.rs from assets/logo/; do not edit. */\n\
          #include \"logo.h\"\n";
    let mut starts: Vec<usize> = bases.clone();
    starts.sort_unstable();
    starts.dedup();
    let pool_name = |base: usize| {
        let first = bases
            .iter()
            .position(|&b| b == base)
            .expect("a pool's first logo");
        format!(
            "{}_RUNS",
            LOGOS[first].name.replace('-', "_").to_uppercase()
        )
    };
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(data.len());
        let _ = write!(
            s,
            "\nstatic const uint8_t {}[{}] = {{\n{}}};\n",
            pool_name(start),
            end - start,
            table(&data[start..end], 16, |v| format!("0x{v:02x}")),
        );
    }
    for (((l, img), r), &base) in LOGOS.iter().zip(&imgs).zip(&rows).zip(&bases) {
        let rows_name = l.name.replace('-', "_").to_uppercase();
        let _ = write!(
            s,
            "\nstatic const uint16_t {rows_name}[{}] = {{\n{}}};\n\n\
             const grid_bitmap {} =\n    {{{}, {}, {rows_name}, {}}};\n",
            img.h,
            table(r, 12, |v| format!("{v:5}")),
            l.symbol,
            img.w,
            img.h,
            pool_name(base),
        );
    }
    s
}

pub fn check_or_write(update: bool) -> Vec<PathBuf> {
    check_or_write_file(&generated_path(), &c_source(), update)
}

pub(crate) fn check_or_write_file(
    path: &std::path::Path,
    want: &str,
    update: bool,
) -> Vec<PathBuf> {
    if update {
        std::fs::write(path, want).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        return Vec::new();
    }
    if std::fs::read_to_string(path).ok().as_deref() == Some(want) {
        Vec::new()
    } else {
        vec![path.to_path_buf()]
    }
}
