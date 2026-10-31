// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod digi;
pub mod filter_fit;
pub mod profiles;

use std::fmt::Write;
use std::path::{Path, PathBuf};

use filter_fit::FilterFit;
use residfp_sys::{pulldown_table, CombinedWaveforms, Model, PULLDOWN_ROWS};

pub const RESIDFP_REV: &str = "c1791383";

pub struct Generated {
    pub path: &'static str,
    pub contents: String,
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repository root")
}

fn pack12(row: &[u16; 4096]) -> Vec<u8> {
    row.chunks(2)
        .flat_map(|p| {
            let (a, b) = (p[0], p[1]);
            assert!(a < 0x1000 && b < 0x1000);
            [
                (a & 0xff) as u8,
                ((a >> 8) | ((b & 0x0f) << 4)) as u8,
                (b >> 4) as u8,
            ]
        })
        .collect()
}

pub const PROFILES: [(CombinedWaveforms, &str); 3] = [
    (CombinedWaveforms::Weak, "WEAK"),
    (CombinedWaveforms::Average, "AVERAGE"),
    (CombinedWaveforms::Strong, "STRONG"),
];

pub const PROFILE_BYTES: usize = 5 * 6144;

pub fn all_profiles_flag(model: Model) -> &'static str {
    match model {
        Model::Mos6581 => "SIDOU_ALL_COMBINED_PROFILES",
        Model::Csg8580 => "SIDOU_ALL_COMBINED_PROFILES_8580",
    }
}

pub const PD_BLOCK: usize = 16;
pub const PD_BLOCK_BYTES: usize = PD_BLOCK * 3 / 2;
pub const PD_INDEX_BYTES: usize = 4096 / PD_BLOCK * 2;

pub fn compiled_profiles(all: bool) -> Vec<(CombinedWaveforms, &'static str)> {
    PROFILES
        .iter()
        .copied()
        .filter(|&(cws, _)| all || cws == CombinedWaveforms::Average)
        .collect()
}

pub struct PulldownSource {
    pub mos6581: Vec<Vec<[u16; 4096]>>,
    pub csg8580: Vec<Vec<[u16; 4096]>>,
}

impl PulldownSource {
    pub fn new() -> Self {
        let all = |m| {
            PROFILES
                .iter()
                .map(|&(c, _)| pulldown_table(m, c))
                .collect()
        };
        Self {
            mos6581: all(Model::Mos6581),
            csg8580: all(Model::Csg8580),
        }
    }

    pub fn profile(&self, model: Model, cws: CombinedWaveforms) -> &[[u16; 4096]] {
        let i = PROFILES.iter().position(|p| p.0 == cws).unwrap();
        match model {
            Model::Mos6581 => &self.mos6581[i],
            Model::Csg8580 => &self.csg8580[i],
        }
    }

    pub fn compiled_rows(&self, all_6581: bool, all_8580: bool) -> Vec<[u16; 4096]> {
        let mut rows = Vec::new();
        for (model, all) in [(Model::Mos6581, all_6581), (Model::Csg8580, all_8580)] {
            for (cws, _) in compiled_profiles(all) {
                rows.extend_from_slice(self.profile(model, cws));
            }
        }
        rows
    }
}

impl Default for PulldownSource {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PulldownLayout {
    pub rows: usize,
    pub blocks: usize,
    pub row_map: Vec<usize>,
    pub bytes: Vec<u8>,
}

fn overlap_blocks(blocks: &[Vec<u8>]) -> (Vec<u8>, Vec<usize>) {
    let n = blocks.len();
    let len = PD_BLOCK_BYTES;
    let mut next_block_and_overlap: Vec<Option<(usize, usize)>> = vec![None; n];
    let mut has_prev = vec![false; n];
    let mut head_of: Vec<usize> = (0..n).collect();
    let mut tail_of: Vec<usize> = (0..n).collect();
    for l in (1..len).rev() {
        let mut by_prefix: std::collections::HashMap<&[u8], Vec<usize>> =
            std::collections::HashMap::new();
        for j in (0..n).rev() {
            if !has_prev[j] {
                by_prefix.entry(&blocks[j][..l]).or_default().push(j);
            }
        }
        for i in 0..n {
            if next_block_and_overlap[i].is_some() {
                continue;
            }
            let Some(heads) = by_prefix.get_mut(&blocks[i][len - l..]) else {
                continue;
            };
            let pos = heads.iter().rposition(|&j| !has_prev[j] && j != head_of[i]);
            let Some(pos) = pos else {
                continue;
            };
            let j = heads.remove(pos);
            next_block_and_overlap[i] = Some((j, l));
            has_prev[j] = true;
            let (h, t) = (head_of[i], tail_of[j]);
            tail_of[h] = t;
            head_of[t] = h;
        }
    }
    let mut out = Vec::new();
    let mut at = vec![0; n];
    for h in (0..n).filter(|&h| !has_prev[h]) {
        at[h] = out.len();
        out.extend_from_slice(&blocks[h]);
        let mut b = h;
        while let Some((c, l)) = next_block_and_overlap[b] {
            at[c] = out.len() - l;
            out.extend_from_slice(&blocks[c][l..]);
            b = c;
        }
    }
    (out, at)
}

impl PulldownLayout {
    pub fn new(rows: &[[u16; 4096]]) -> Self {
        let mut unique: Vec<&[u16; 4096]> = Vec::new();
        let mut row_map = Vec::with_capacity(rows.len());
        for row in rows {
            let r = unique.iter().position(|u| *u == row).unwrap_or_else(|| {
                unique.push(row);
                unique.len() - 1
            });
            row_map.push(r);
        }
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        let mut seen = std::collections::HashMap::new();
        let mut uses = Vec::with_capacity(unique.len() * 256);
        for row in &unique {
            for block in row.chunks(PD_BLOCK) {
                let b = *seen.entry(block.to_vec()).or_insert_with(|| {
                    let mut full = [0u16; 4096];
                    full[..PD_BLOCK].copy_from_slice(block);
                    blocks.push(pack12(&full)[..PD_BLOCK_BYTES].to_vec());
                    blocks.len() - 1
                });
                uses.push(b);
            }
        }
        let (pool, at) = overlap_blocks(&blocks);
        let pool_at = unique.len() * PD_INDEX_BYTES;
        let mut bytes = Vec::with_capacity(pool_at + pool.len());
        for (k, &b) in uses.iter().enumerate() {
            let r = k / 256;
            let offset = pool_at + at[b] - r * PD_INDEX_BYTES;
            let offset = u16::try_from(offset).expect("a pulldown block beyond a u16 offset");
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.extend_from_slice(&pool);
        Self {
            rows: unique.len(),
            blocks: blocks.len(),
            row_map,
            bytes,
        }
    }

    pub fn index(&self, r: usize, k: usize) -> u16 {
        let at = r * PD_INDEX_BYTES + k * 2;
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    pub fn pool(&self) -> &[u8] {
        &self.bytes[self.rows * PD_INDEX_BYTES..]
    }
}

pub fn pulldown_entry(sid_pd: &[u8], r: usize, x: usize) -> u16 {
    let row = r * PD_INDEX_BYTES;
    let k = row + (x >> 4) * 2;
    let offset = usize::from(u16::from_le_bytes([sid_pd[k], sid_pd[k + 1]]));
    let p = &sid_pd[row + offset + ((x >> 1) & 7) * 3..];
    if x & 1 == 1 {
        u16::from(p[1] >> 4) | (u16::from(p[2]) << 4)
    } else {
        u16::from(p[0]) | (u16::from(p[1] & 0x0f) << 8)
    }
}

pub const PULLDOWN_SETS: [(bool, bool); 4] =
    [(true, false), (true, true), (false, false), (false, true)];

fn pulldown_header(src: &PulldownSource) -> Generated {
    let mut s = String::new();
    // REUSE-IgnoreStart
    s.push_str(
        "// SPDX-FileCopyrightText: 2011-2026 Leandro Nini <drfiemost@users.sourceforge.net>\n\
         // SPDX-FileCopyrightText: 2007-2010 Antti Lankila\n\
         // SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
         // SPDX-License-Identifier: GPL-2.0-or-later\n",
    );
    // REUSE-IgnoreEnd
    writeln!(
        s,
        "/* Generated by sidou-tablegen; do not edit. To regenerate, from host/:\n\
         \x20*   cargo run --release -p sidou-tablegen\n\
         \x20* Source: reSIDfp (libresidfp {RESIDFP_REV}),\n\
         \x20* WaveformCalculator::buildPulldownTable(model, profile): the combined-waveform\n\
         \x20* pulldown, which maps a 12-bit waveform output (the AND of the selected\n\
         \x20* waveforms) to what the chip outputs, five rows a profile in reSIDfp's order\n\
         \x20* ({}): the 6581's compiled profiles (gen/grid.h), then the 8580's from row\n\
         \x20* SID_PD_8580_ROW.\n\
         \x20*\n\
         \x20* Lossless and shared: a row is 256 blocks of 16 entries, and every distinct block\n\
         \x20* is stored once, in `pool`, {PD_BLOCK_BYTES} bytes packing its entries two to three bytes;\n\
         \x20* the blocks overlap there wherever one ends with the bytes the next begins with.\n\
         \x20* Entry x of index row r is in the block that starts idx[r][x >> 4] bytes after\n\
         \x20* &idx[r][0]; sid_pulldown() in sid_osc.h unpacks it. Rows with the same entries\n\
         \x20* share an index row (each model's NP rows): SID_PD_ROWS_INIT lists each compiled\n\
         \x20* row's, five a profile, for SID_PD_ROWS (sid.c). Unshared, a profile would take\n\
         \x20* {PROFILE_BYTES} bytes a model. By the models' flags:",
        PULLDOWN_ROWS.join(", "),
    )
    .unwrap();
    let layouts: Vec<PulldownLayout> = PULLDOWN_SETS
        .iter()
        .map(|&(a, b)| PulldownLayout::new(&src.compiled_rows(a, b)))
        .collect();
    let condition = |(a, b): (bool, bool)| {
        let def = |on: bool, model| {
            format!(
                "{}defined({})",
                if on { "" } else { "!" },
                all_profiles_flag(model)
            )
        };
        format!("{} && {}", def(a, Model::Mos6581), def(b, Model::Csg8580))
    };
    for (&set, l) in PULLDOWN_SETS.iter().zip(&layouts) {
        writeln!(
            s,
            " *   {}: {} rows ({} index rows), {} blocks, {} bytes ({} unshared)",
            condition(set),
            l.row_map.len(),
            l.rows,
            l.blocks,
            l.bytes.len(),
            l.row_map.len() / 5 * PROFILE_BYTES
        )
        .unwrap();
    }
    writeln!(
        s,
        " */\n#ifndef SIDOU_GEN_PULLDOWN_H\n#define SIDOU_GEN_PULLDOWN_H\n\n\
         #include <stdint.h>\n\n#include \"grid.h\"\n\n\
         /* The 8580's first row in SID_PD_ROWS_INIT. */\n\
         #define SID_PD_8580_ROW (5 * SID_COMBINED_TABLES_6581)\n"
    )
    .unwrap();
    for (n, (&set, l)) in PULLDOWN_SETS.iter().zip(&layouts).enumerate() {
        let directive = if n == 0 { "#if" } else { "#elif" };
        writeln!(s, "{directive} {}", condition(set)).unwrap();
        writeln!(
            s,
            "static const struct {{\n    uint16_t idx[{}][256];\n    uint8_t pool[{}];\n}} SID_PD = {{\n    {{",
            l.rows,
            l.pool().len()
        )
        .unwrap();
        for r in 0..l.rows {
            writeln!(s, "        {{").unwrap();
            let entries: Vec<String> = (0..256).map(|k| l.index(r, k).to_string()).collect();
            for line in entries.chunks(16) {
                writeln!(s, "            {},", line.join(", ")).unwrap();
            }
            writeln!(s, "        }},").unwrap();
        }
        writeln!(s, "    }},\n    {{").unwrap();
        for block in l.pool().chunks(PD_BLOCK_BYTES) {
            let bytes: Vec<String> = block.iter().map(|b| format!("0x{b:02x}")).collect();
            writeln!(s, "        {},", bytes.join(", ")).unwrap();
        }
        writeln!(s, "    }},\n}};").unwrap();
        let rows: Vec<String> = l
            .row_map
            .iter()
            .map(|r| format!("SID_PD.idx[{r}]"))
            .collect();
        writeln!(s, "#define SID_PD_ROWS_INIT \\").unwrap();
        for (n, line) in rows.chunks(5).enumerate() {
            let open = if n == 0 { "{" } else { " " };
            let close = if (n + 1) * 5 >= rows.len() {
                "}"
            } else {
                ", \\"
            };
            writeln!(s, "    {open}{}{close}", line.join(", ")).unwrap();
        }
    }
    writeln!(s, "#endif\n\n#endif").unwrap();
    Generated {
        path: "core/sid/gen/pulldown.h",
        contents: s,
    }
}

fn grid_header() -> Generated {
    let mut s = String::new();
    // REUSE-IgnoreStart
    s.push_str(
        "// SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
         // SPDX-License-Identifier: GPL-3.0-or-later\n",
    );
    // REUSE-IgnoreEnd
    let names: Vec<&str> = PROFILES.iter().map(|p| p.1).collect();
    writeln!(
        s,
        "/* Generated by sidou-tablegen; do not edit. Which generated tables are compiled.\n\
         \x20*\n\
         \x20* Combined-waveform profiles: reSIDfp's {} sampled chips. Per model,\n\
         \x20* only AVERAGE, reSIDfp's default, is compiled (gen/pulldown.h) unless the\n\
         \x20* model's flag is defined (all three): {} for the 6581,\n\
         \x20* {} for the 8580. */\n\
         #ifndef SIDOU_GEN_GRID_H\n#define SIDOU_GEN_GRID_H\n",
        names.join(", "),
        all_profiles_flag(Model::Mos6581),
        all_profiles_flag(Model::Csg8580),
    )
    .unwrap();
    let enumerators: Vec<String> = names.iter().map(|n| format!("SID_PROFILE_{n}")).collect();
    writeln!(s, "enum {{ {}, SID_PROFILES }};\n", enumerators.join(", ")).unwrap();
    writeln!(
        s,
        "/* A model's table index for a profile; a profile not compiled in gets AVERAGE's. */"
    )
    .unwrap();
    for (model, name) in [(Model::Mos6581, "6581"), (Model::Csg8580, "8580")] {
        writeln!(
            s,
            "#ifdef {flag}\n\
             #define SID_COMBINED_TABLES_{name} {n}\n\
             #define SID_COMBINED_TABLE_{name}(profile) (profile)\n\
             #else\n\
             #define SID_COMBINED_TABLES_{name} 1\n\
             #define SID_COMBINED_TABLE_{name}(profile) 0\n\
             #endif",
            flag = all_profiles_flag(model),
            n = PROFILES.len()
        )
        .unwrap();
    }
    writeln!(s, "\n#endif").unwrap();
    Generated {
        path: "core/sid/gen/grid.h",
        contents: s,
    }
}

fn noise_shift(sr: u32) -> u32 {
    (sr >> 1) | (((sr ^ (sr >> 5)) & 1) << 22)
}

pub fn noise_jump_matrices() -> [[u32; 23]; 23] {
    let mut m = [[0u32; 23]; 23];
    let mut cols: Vec<u32> = (0..23).map(|i| noise_shift(1 << i)).collect();
    for row in &mut m {
        row.copy_from_slice(&cols);
        let apply = |m: &[u32], v: u32| -> u32 {
            (0..23)
                .filter(|i| v >> i & 1 != 0)
                .fold(0, |acc, i| acc ^ m[i])
        };
        cols = cols.iter().map(|&c| apply(&cols, c)).collect();
    }
    m
}

pub use profiles::FILTER_CURVES;

pub fn filter_fits() -> Vec<FilterFit> {
    FILTER_CURVES.iter().map(filter_fit::fit_setting).collect()
}

fn c_floats(s: &mut String, values: &[f64], decimals: usize, per_line: usize) {
    for line in values.chunks(per_line) {
        let v: Vec<String> = line.iter().map(|x| format!("{x:.decimals$}f")).collect();
        writeln!(s, "        {},", v.join(", ")).unwrap();
    }
}

const CUTOFF_CODE_BIAS: u32 = 126;

fn cutoff_code(hz: f64) -> u16 {
    let hz = format!("{hz:.1}").parse::<f32>().unwrap().max(0.5);
    let code = (hz.to_bits() - (CUTOFF_CODE_BIAS << 23) + (1 << 10)) >> 11;
    let code = u16::try_from(code).expect("cutoff fits in a 16-bit code");
    let decoded = f32::from_bits((u32::from(code) << 11) + (CUTOFF_CODE_BIAS << 23));
    assert!((f64::from(decoded) / f64::from(hz) - 1.0).abs() <= 2.5e-4);
    code
}

pub const CUTOFF_BLOCK: usize = 32;
pub const CUTOFF_ESCAPE: u8 = 0x80;

pub fn delta_code_cutoffs(curves: &[Vec<u16>]) -> (Vec<u16>, Vec<u8>) {
    let (mut at, mut run) = (Vec::new(), Vec::new());
    for codes in curves {
        assert_eq!(codes.len(), 2048);
        for block in codes.chunks(CUTOFF_BLOCK) {
            at.push(u16::try_from(run.len()).expect("the cutoff stream beyond a u16 offset"));
            run.extend_from_slice(&block[0].to_be_bytes());
            for w in block.windows(2) {
                let d = i32::from(w[1]) - i32::from(w[0]);
                if (-127..=127).contains(&d) {
                    run.push(d as i8 as u8);
                } else {
                    run.push(CUTOFF_ESCAPE);
                    run.extend_from_slice(&w[1].to_be_bytes());
                }
            }
        }
    }
    (at, run)
}

pub fn cutoff_code_at(at: &[u16], run: &[u8], curve: usize, fc: usize) -> u16 {
    let mut p = usize::from(at[curve * (2048 / CUTOFF_BLOCK) + fc / CUTOFF_BLOCK]);
    let mut code = u16::from_be_bytes([run[p], run[p + 1]]);
    p += 2;
    for _ in 0..fc % CUTOFF_BLOCK {
        if run[p] == CUTOFF_ESCAPE {
            code = u16::from_be_bytes([run[p + 1], run[p + 2]]);
            p += 3;
        } else {
            code = code.wrapping_add(run[p] as i8 as u16);
            p += 1;
        }
    }
    code
}

pub fn filter_header(fits: &[FilterFit]) -> Generated {
    let mut s = String::new();
    // REUSE-IgnoreStart
    s.push_str(
        "// SPDX-FileCopyrightText: 2011-2026 Leandro Nini <drfiemost@users.sourceforge.net>\n\
         // SPDX-FileCopyrightText: 2007-2010 Antti Lankila\n\
         // SPDX-FileCopyrightText: 2004,2010 Dag Lem <resid@nimrod.no>\n\
         // SPDX-FileCopyrightText: 2026 SIDOU contributors\n\
         // SPDX-License-Identifier: GPL-2.0-or-later\n",
    );
    // REUSE-IgnoreEnd
    writeln!(
        s,
        "/* Generated by sidou-tablegen; do not edit. To regenerate, from host/:\n\
         \x20*   cargo run --release -p sidou-tablegen -- fit-filter\n\
         \x20* Source: reSIDfp's filter (libresidfp {RESIDFP_REV}), measured and fitted by\n\
         \x20* sidou-tablegen (filter_fit.rs): noise through the filter at a small level against\n\
         \x20* the same noise unfiltered, LP, BP and HP, FC every 32 steps (and 1023 on the 6581)\n\
         \x20* at resonance 0, 8 and 15, every resonance at six FC values mid-range; SIDOU's\n\
         \x20* state-variable filter fitted to each around its cutoff. Data derived from\n\
         \x20* reSIDfp's output, so under its licence.\n\
         \x20*\n\
         \x20* The cutoff codes, uint16_t for each FC value of each curve setting, interpolated\n\
         \x20* between the measured ones along what the chip's cutoff DAC outputs, so it is\n\
         \x20* monotonic where reSIDfp's is (the 6581's DAC is kinked: 1023 -> 1024 drops).\n\
         \x20* Out of the measurement's reach (below about 40 Hz, above about 15 kHz) it is\n\
         \x20* extrapolated. Float bits, rounded to the top 12 mantissa bits, with the exponent\n\
         \x20* rebased to 0.5 Hz (smaller cutoffs clamp there; the filter clamps below 30 Hz):\n\
         \x20* bits = (code << 11) + (SID_CUTOFF_CODE_BIAS << 23), within 2.5e-4 relative.\n\
         \x20* Delta-coded in blocks of 32 FC values: block b of curve c starts at byte\n\
         \x20* SID_CUTOFF_AT_*[c][b] of SID_CUTOFF_RUN_*, with its first code (big-endian), then\n\
         \x20* each next code as a signed byte, its difference from the code before, or 0x80 and\n\
         \x20* the code itself (big-endian) where the difference is beyond a signed byte.\n\
         \x20* Decoded in the main loop (sid_filter.h, sid_cutoff_seq_hz), never the audio\n\
         \x20* interrupt.\n\
         \x20* SID_RES_K_*[curve][res]: k = 1/Q for each resonance value.\n\
         \x20* Curve settings, by index:"
    )
    .unwrap();
    for model in [Model::Mos6581, Model::Csg8580] {
        for (i, f) in fits.iter().filter(|f| f.setting.model == model).enumerate() {
            writeln!(s, " *   {i}: {}", f.setting.describe()).unwrap();
        }
    }
    writeln!(
        s,
        " */\n#ifndef SIDOU_GEN_FILTER_H\n#define SIDOU_GEN_FILTER_H\n\n\
         #include <stdint.h>\n\n#define SID_CUTOFF_CODE_BIAS {CUTOFF_CODE_BIAS}u\n"
    )
    .unwrap();
    for (model, name) in [(Model::Mos6581, "6581"), (Model::Csg8580, "8580")] {
        let mine: Vec<&FilterFit> = fits.iter().filter(|f| f.setting.model == model).collect();
        writeln!(s, "#define SID_FILTER_CURVES_{name} {}\n", mine.len()).unwrap();
        let codes: Vec<Vec<u16>> = mine
            .iter()
            .map(|f| f.cutoff_hz.iter().map(|&hz| cutoff_code(hz)).collect())
            .collect();
        let (at, run) = delta_code_cutoffs(&codes);
        writeln!(
            s,
            "static const uint16_t SID_CUTOFF_AT_{name}[SID_FILTER_CURVES_{name}][{}] = {{",
            2048 / CUTOFF_BLOCK
        )
        .unwrap();
        for (f, offsets) in mine.iter().zip(at.chunks(2048 / CUTOFF_BLOCK)) {
            writeln!(s, "    /* {} */ {{", f.setting.describe()).unwrap();
            for line in offsets.chunks(16) {
                let v: Vec<String> = line.iter().map(u16::to_string).collect();
                writeln!(s, "        {},", v.join(", ")).unwrap();
            }
            writeln!(s, "    }},").unwrap();
        }
        writeln!(s, "}};\n").unwrap();
        writeln!(
            s,
            "static const uint8_t SID_CUTOFF_RUN_{name}[{}] = {{",
            run.len()
        )
        .unwrap();
        for line in run.chunks(16) {
            let v: Vec<String> = line.iter().map(|b| format!("0x{b:02x}")).collect();
            writeln!(s, "    {},", v.join(", ")).unwrap();
        }
        writeln!(s, "}};\n").unwrap();
        writeln!(
            s,
            "static const float SID_RES_K_{name}[SID_FILTER_CURVES_{name}][16] = {{"
        )
        .unwrap();
        for f in &mine {
            writeln!(s, "    /* {} */ {{", f.setting.describe()).unwrap();
            c_floats(&mut s, &f.k, 4, 8);
            writeln!(s, "    }},").unwrap();
        }
        writeln!(s, "}};\n").unwrap();
    }
    writeln!(s, "#endif").unwrap();
    Generated {
        path: "core/sid/gen/filter.h",
        contents: s,
    }
}

pub fn generate_computed() -> Vec<Generated> {
    vec![
        grid_header(),
        profiles::profiles_header(),
        pulldown_header(&PulldownSource::new()),
        digi::digi_header(),
    ]
}

pub fn generate() -> Vec<Generated> {
    let mut all = generate_computed();
    all.push(filter_header(&filter_fits()));
    all
}

fn float_decimals(token: &str) -> Option<(f64, usize)> {
    let body = token.strip_suffix('f')?;
    let (int, frac) = body.split_once('.')?;
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    if !digits(int.strip_prefix('-').unwrap_or(int)) || !digits(frac) {
        return None;
    }
    Some((body.parse().ok()?, frac.len()))
}

pub fn differs_beyond_last_digit(a: &str, b: &str) -> Option<String> {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    if la.len() != lb.len() {
        return Some(format!("{} lines against {}", la.len(), lb.len()));
    }
    for (n, (x, y)) in la.iter().zip(&lb).enumerate() {
        let tx: Vec<&str> = x.split([' ', ',']).filter(|t| !t.is_empty()).collect();
        let ty: Vec<&str> = y.split([' ', ',']).filter(|t| !t.is_empty()).collect();
        let same = tx.len() == ty.len()
            && tx.iter().zip(&ty).all(|(p, q)| {
                p == q
                    || matches!(
                        (float_decimals(p), float_decimals(q)),
                        (Some((u, d)), Some((v, e)))
                            if d == e && (u - v).abs() <= 10f64.powi(-(d as i32)) * 1.000_001
                    )
            });
        if !same {
            return Some(format!("line {}: {x:?} against {y:?}", n + 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_codes_clamp_and_round_across_exponents() {
        for hz in [0.0, 0.1, 0.4, 0.5] {
            assert_eq!(cutoff_code(hz), 0);
        }
        assert_eq!(cutoff_code(1.0), 4096);
        assert_eq!(cutoff_code(8191.5), 57344);
        assert_eq!(cutoff_code(8192.0), 57344);
        assert_eq!(cutoff_code(8193.0), 57345);
    }

    #[test]
    fn delta_coded_cutoffs_read_back() {
        let mut a: Vec<u16> = (0..2048).map(|i| (i * 13 % 251) as u16 + 1000).collect();
        a[40] = 60000;
        a[41] = 3;
        a[64] = 65535;
        a[65] = 65408;
        let b: Vec<u16> = (0..2048).map(|i| 30000 - (i as u16) * 7).collect();
        let (at, run) = delta_code_cutoffs(&[a.clone(), b.clone()]);
        assert_eq!(at.len(), 2 * 2048 / CUTOFF_BLOCK);
        for (c, codes) in [a, b].iter().enumerate() {
            for (fc, &want) in codes.iter().enumerate() {
                assert_eq!(cutoff_code_at(&at, &run, c, fc), want, "curve {c} FC {fc}");
            }
        }
    }

    #[test]
    fn float_tables_compare_up_to_the_last_digit() {
        let a = "    /* 0: x */ {\n        440.1f, 2.0000f, -0.5f,\n";
        assert_eq!(differs_beyond_last_digit(a, a), None);
        let last = "    /* 0: x */ {\n        440.2f, 1.9999f, -0.4f,\n";
        assert_eq!(differs_beyond_last_digit(a, last), None);
        for b in [
            "    /* 0: x */ {\n        440.3f, 2.0000f, -0.5f,\n",
            "    /* 0: x */ {\n        440.1f, 2.0002f, -0.5f,\n",
            "    /* 0: x */ {\n        440.1f, 2.000f, -0.5f,\n",
            "    /* 1: x */ {\n        440.1f, 2.0000f, -0.5f,\n",
            "    /* 0: x */ {\n        440.1f, 2.0000f,\n",
            "    /* 0: x */ {\n        440.1f, 2.0000f, -0.5f,\n\n",
        ] {
            assert!(differs_beyond_last_digit(a, b).is_some(), "{b:?}");
        }
    }

    #[test]
    fn shared_pulldown_blocks_read_back_as_residfps_tables() {
        let src = PulldownSource::new();
        for (all_6581, all_8580) in PULLDOWN_SETS {
            let l = PulldownLayout::new(&src.compiled_rows(all_6581, all_8580));
            let mut r = 0;
            for (model, all) in [(Model::Mos6581, all_6581), (Model::Csg8580, all_8580)] {
                for (cws, label) in compiled_profiles(all) {
                    for (row, row_label) in src.profile(model, cws).iter().zip(PULLDOWN_ROWS) {
                        for (x, &want) in row.iter().enumerate() {
                            assert_eq!(
                                pulldown_entry(&l.bytes, l.row_map[r], x),
                                want,
                                "{model:?} {label} {row_label} entry {x} ({all_6581}, {all_8580})"
                            );
                        }
                        r += 1;
                    }
                }
            }
            assert_eq!(r, l.row_map.len());
            let index_rows: std::collections::HashSet<usize> = l.row_map.iter().copied().collect();
            assert_eq!(index_rows.len(), l.rows, "an index row no row uses");
            let used: std::collections::HashSet<usize> = (0..l.rows)
                .flat_map(|r| (0..256).map(move |k| (r, k)))
                .map(|(r, k)| r * PD_INDEX_BYTES + usize::from(l.index(r, k)))
                .collect();
            assert_eq!(used.len(), l.blocks, "a block no row uses");
            let end = used.iter().max().unwrap() + PD_BLOCK_BYTES;
            assert_eq!(end, l.bytes.len(), "pool bytes past the last block");
        }
        let device = PulldownLayout::new(&src.compiled_rows(true, false));
        assert_eq!(device.row_map.len(), 20);
        assert!(
            device.bytes.len() < 4 * PROFILE_BYTES / 2,
            "{}",
            device.bytes.len()
        );
    }

    #[test]
    fn overlapped_blocks_are_where_their_offsets_say() {
        let blocks: Vec<Vec<u8>> = (0..200u32)
            .map(|b| {
                (0..PD_BLOCK_BYTES as u32)
                    .map(|i| ((b * 7 + i) % 13) as u8)
                    .collect()
            })
            .collect();
        let mut distinct = blocks.clone();
        distinct.sort();
        distinct.dedup();
        let (pool, at) = overlap_blocks(&distinct);
        assert!(pool.len() < distinct.len() * PD_BLOCK_BYTES);
        for (b, &a) in distinct.iter().zip(&at) {
            assert_eq!(&pool[a..a + PD_BLOCK_BYTES], &b[..]);
        }
    }

    #[test]
    fn pack12_round_trips() {
        let mut row = [0u16; 4096];
        for (i, r) in row.iter_mut().enumerate() {
            *r = ((i * 2654435761) >> 7) as u16 & 0xfff;
        }
        let p = pack12(&row);
        assert_eq!(p.len(), 6144);
        for (i, &v) in row.iter().enumerate() {
            let b = &p[(i / 2) * 3..];
            let got = if i % 2 == 0 {
                u16::from(b[0]) | (u16::from(b[1] & 0x0f) << 8)
            } else {
                u16::from(b[1] >> 4) | (u16::from(b[2]) << 4)
            };
            assert_eq!(got, v, "entry {i}");
        }
    }
}
