// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Generates SIDOU's checked-in C tables in `core/sid/gen/` from reSIDfp (test-only, never
//! linked into firmware). The tables are data derived from reSIDfp, so the generated files
//! carry its licence (GPL-2.0-or-later) and its authors.
//!
//! `cargo run --release -p sidou-tablegen` (from `host/`) rewrites them; the drift test
//! fails when the checked-in files differ from what this produces.

pub mod digi;
pub mod filter_fit;
pub mod profiles;

use std::fmt::Write;
use std::path::{Path, PathBuf};

use filter_fit::FilterFit;
use residfp_sys::{pulldown_table, CombinedWaveforms, Model, PULLDOWN_ROWS};

/// The libresidfp revision the tables come from (the third_party/libresidfp submodule).
pub const RESIDFP_REV: &str = "c1791383";

/// One generated file: its path relative to the repository root, and its contents.
pub struct Generated {
    pub path: &'static str,
    pub contents: String,
}

/// The repository root, from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repository root")
}

/// Packs 12-bit entries two to three bytes: e0 low 8 bits; e0 high 4 | e1 low 4 << 4;
/// e1 high 8 bits.
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

/// The combined-waveform profiles: reSIDfp's three sampled chips, in the order of the
/// C enum in gen/grid.h. AVERAGE is reSIDfp's default and the only one compiled for a
/// model unless its flag (`all_profiles_flag`) is defined.
pub const PROFILES: [(CombinedWaveforms, &str); 3] = [
    (CombinedWaveforms::Weak, "WEAK"),
    (CombinedWaveforms::Average, "AVERAGE"),
    (CombinedWaveforms::Strong, "STRONG"),
];

/// Bytes of one profile's table for one model, packed but not shared: what a profile
/// would cost without the shared blocks (gen/pulldown.h).
pub const PROFILE_BYTES: usize = 5 * 6144;

/// The define that compiles all three of a model's combined-waveform profiles, not just
/// AVERAGE: each model has its own, since the device's chip profiles play WEAK and STRONG
/// on the 6581 only (gen/profiles.h), and the 8580's would sit unused.
pub fn all_profiles_flag(model: Model) -> &'static str {
    match model {
        Model::Mos6581 => "SIDOU_ALL_COMBINED_PROFILES",
        Model::Csg8580 => "SIDOU_ALL_COMBINED_PROFILES_8580",
    }
}

/// Entries in one block of a pulldown row, and its bytes packed (pack12).
pub const PD_BLOCK: usize = 16;
pub const PD_BLOCK_BYTES: usize = PD_BLOCK * 3 / 2;
/// Bytes of one row's index: a u16 for each of its blocks.
pub const PD_INDEX_BYTES: usize = 4096 / PD_BLOCK * 2;

/// The profiles compiled for a model, in table order: all three with its flag, else
/// AVERAGE alone.
pub fn compiled_profiles(all: bool) -> Vec<(CombinedWaveforms, &'static str)> {
    PROFILES
        .iter()
        .copied()
        .filter(|&(cws, _)| all || cws == CombinedWaveforms::Average)
        .collect()
}

/// reSIDfp's pulldown rows for every profile of both models, PROFILES order.
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

    /// One profile's five rows for a model.
    pub fn profile(&self, model: Model, cws: CombinedWaveforms) -> &[[u16; 4096]] {
        let i = PROFILES.iter().position(|p| p.0 == cws).unwrap();
        match model {
            Model::Mos6581 => &self.mos6581[i],
            Model::Csg8580 => &self.csg8580[i],
        }
    }

    /// The rows SID_PD holds with the flags given: the 6581's compiled profiles, then the
    /// 8580's, five rows each.
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

/// SID_PD's bytes as the device holds them (gen/pulldown.h): each distinct row's index,
/// PD_INDEX_BYTES of little-endian u16s, then the pool of distinct blocks, PD_BLOCK_BYTES
/// each. Index entry k of index row r is the offset from that row's first byte to the block
/// holding the row's entries 16k .. 16k + 15, so the audio path needs no other base address.
/// Rows with the same entries share one index row (the 6581's three NP rows are alike, and
/// so are the 8580's): `row_map` gives each compiled row's. The pool is one string in which
/// the blocks overlap wherever one ends with the bytes another begins with (greedy, longest
/// overlap first), since the offsets are byte offsets.
pub struct PulldownLayout {
    pub rows: usize,
    pub blocks: usize,
    pub row_map: Vec<usize>,
    pub bytes: Vec<u8>,
}

/// The blocks laid out as one string, each overlapping the one before it as far as their
/// bytes agree: greedy merging of chains, the longest overlap first (ties in block order).
/// Returns the string and each block's offset in it.
fn overlap_blocks(blocks: &[Vec<u8>]) -> (Vec<u8>, Vec<usize>) {
    let n = blocks.len();
    let len = PD_BLOCK_BYTES;
    let mut next: Vec<Option<(usize, usize)>> = vec![None; n]; // (block, overlap)
    let mut has_prev = vec![false; n];
    // For a chain's tail, its head; for a chain's head, its tail.
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
            if next[i].is_some() {
                continue;
            }
            let Some(heads) = by_prefix.get_mut(&blocks[i][len - l..]) else {
                continue;
            };
            // The first free head that is not this chain's own (no cycles); the others stay.
            let pos = heads.iter().rposition(|&j| !has_prev[j] && j != head_of[i]);
            let Some(pos) = pos else {
                continue;
            };
            let j = heads.remove(pos);
            next[i] = Some((j, l));
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
        while let Some((c, l)) = next[b] {
            at[c] = out.len() - l;
            out.extend_from_slice(&blocks[c][l..]);
            b = c;
        }
    }
    (out, at)
}

impl PulldownLayout {
    pub fn new(rows: &[[u16; 4096]]) -> Self {
        // The distinct rows, and the distinct blocks in the order they first appear.
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

    /// Index entry k of index row r.
    pub fn index(&self, r: usize, k: usize) -> u16 {
        let at = r * PD_INDEX_BYTES + k * 2;
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    /// The pool, after the indexes.
    pub fn pool(&self) -> &[u8] {
        &self.bytes[self.rows * PD_INDEX_BYTES..]
    }
}

/// Entry x of index row r of SID_PD's bytes, as sid_pulldown() in sid_osc.h reads it.
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

/// The four combinations of the two models' flags, the device's (the 6581's three, the
/// 8580's AVERAGE) first.
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

/// One plain shift of the noise register (reSIDfp's order, no TEST): up one, the new
/// bit 22 = bit 0 ^ bit 5.
fn noise_shift(sr: u32) -> u32 {
    (sr >> 1) | (((sr ^ (sr >> 5)) & 1) << 22)
}

/// The 2^k-shift maps of the noise register for k = 0..22, as GF(2) matrices: entry [k][i] is
/// the register after 2^k shifts of bit i alone. Our own derivation from the shift, no
/// reSIDfp data. The device builds the same table at boot (SID_NOISE_JUMP, core/sid/sid.c);
/// this is the oracle the host tests hold it to (unit/sid_noise.rs).
pub fn noise_jump_matrices() -> [[u32; 23]; 23] {
    let mut m = [[0u32; 23]; 23];
    let mut cols: Vec<u32> = (0..23).map(|i| noise_shift(1 << i)).collect();
    for row in &mut m {
        row.copy_from_slice(&cols);
        // Square: apply the map to each column again.
        let apply = |m: &[u32], v: u32| -> u32 {
            (0..23)
                .filter(|i| v >> i & 1 != 0)
                .fold(0, |acc, i| acc ^ m[i])
        };
        cols = cols.iter().map(|&c| apply(&cols, c)).collect();
    }
    m
}

/// The filter curve settings fitted, in table order per model (index 0 is the one
/// sid_init uses, reSIDfp's default): the chip profiles' (profiles.rs).
pub use profiles::FILTER_CURVES;

/// Measures and fits every filter curve setting (filter_fit.rs), in FILTER_CURVES order.
/// Each 6581 range setting must be measured on its own (reSIDfp keeps the range once per
/// process), so this runs the settings one after another. The same holds across threads:
/// no other 6581 measurement with another range may run in the process meanwhile (tests in
/// one binary run in parallel threads), unless they share a lock.
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
    // Preserve the measured table's 0.1 Hz precision before encoding its float bits.
    let hz = format!("{hz:.1}").parse::<f32>().unwrap().max(0.5);
    let code = (hz.to_bits() - (CUTOFF_CODE_BIAS << 23) + (1 << 10)) >> 11;
    let code = u16::try_from(code).expect("cutoff fits in a 16-bit code");
    let decoded = f32::from_bits((u32::from(code) << 11) + (CUTOFF_CODE_BIAS << 23));
    assert!((f64::from(decoded) / f64::from(hz) - 1.0).abs() <= 2.5e-4);
    code
}

/// gen/filter.h: per model and curve setting, the cutoff code for each FC value and the
/// damping k = 1/Q for each resonance value.
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
         \x20* SID_CUTOFF_HZ_*[curve][fc]: uint16_t cutoff codes for each FC value, interpolated\n\
         \x20* between the measured ones along what the chip's cutoff DAC outputs, so it is\n\
         \x20* monotonic where reSIDfp's is (the 6581's DAC is kinked: 1023 -> 1024 drops).\n\
         \x20* Out of the measurement's reach (below about 40 Hz, above about 15 kHz) it is\n\
         \x20* extrapolated. Float bits, rounded to the top 12 mantissa bits, with the exponent\n\
         \x20* rebased to 0.5 Hz (smaller cutoffs clamp there; the filter clamps below 30 Hz):\n\
         \x20* bits = (code << 11) + (SID_CUTOFF_CODE_BIAS << 23), within 2.5e-4 relative.\n\
         \x20* Decoded in the main loop, never the audio interrupt.\n\
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
        writeln!(
            s,
            "static const uint16_t SID_CUTOFF_HZ_{name}[SID_FILTER_CURVES_{name}][2048] = {{"
        )
        .unwrap();
        for f in &mine {
            writeln!(s, "    /* {} */ {{", f.setting.describe()).unwrap();
            for line in f.cutoff_hz.chunks(8) {
                let codes: Vec<String> =
                    line.iter().map(|&hz| cutoff_code(hz).to_string()).collect();
                writeln!(s, "        {},", codes.join(", ")).unwrap();
            }
            writeln!(s, "    }},").unwrap();
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

/// The generated files that are computed, not measured: integer tables, exact on every
/// platform, and cheap to make.
pub fn generate_computed() -> Vec<Generated> {
    vec![
        grid_header(),
        profiles::profiles_header(),
        pulldown_header(&PulldownSource::new()),
        digi::digi_header(),
    ]
}

/// Every generated file. The filter tables are measured (about half a minute of CPU).
pub fn generate() -> Vec<Generated> {
    let mut all = generate_computed();
    all.push(filter_header(&filter_fits()));
    all
}

/// The number of decimals of a C float literal as filter_header writes them (`-12.5f`), or
/// None for any other token.
fn float_decimals(token: &str) -> Option<(f64, usize)> {
    let body = token.strip_suffix('f')?;
    let (int, frac) = body.split_once('.')?;
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    if !digits(int.strip_prefix('-').unwrap_or(int)) || !digits(frac) {
        return None;
    }
    Some((body.parse().ok()?, frac.len()))
}

/// The first difference between two generated texts beyond the last printed digit of a
/// float literal: the texts must agree token by token (split at whitespace and commas),
/// except that two float literals with the same number of decimals may differ by one unit
/// in their last place. reSIDfp builds its filter model with the platform's libm (`pow`,
/// `exp`, `log`), and its output resampler too, so a measurement can land on the other
/// side of a rounding boundary on another platform. None if they agree.
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

    /// SID_PD is lossless: under every combination of the flags, each compiled row of
    /// each profile and model reads back, entry for entry, as reSIDfp's table, through its
    /// index row; the device's set holds the 6581's three profiles and the 8580's AVERAGE;
    /// every block in a pool is one a row uses, and every index row one a compiled row uses.
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

    /// Overlapping blocks: each block starts where the layout says, in a string no longer
    /// than the blocks laid end to end, and chains never close on themselves.
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
