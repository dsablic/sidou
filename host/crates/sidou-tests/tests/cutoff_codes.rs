// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use residfp_sys::Model;
use sidou_sys::{
    sid_cutoff_hz, sid_cutoff_seq, sid_cutoff_seq_hz, sid_cutoff_seq_init, sid_filter_curves,
    SID_MODEL_6581, SID_MODEL_8580,
};

fn c_array_numbers(header: &str, name: &str) -> Vec<u32> {
    let at = header
        .find(&format!("{name}["))
        .unwrap_or_else(|| panic!("no {name}"));
    let body = &header[header[at..].find("= {").unwrap() + at + 3..];
    let body = &body[..body.find("};").unwrap()];
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(c) = rest.find("/*") {
        out.extend(numbers(&rest[..c]));
        rest = &rest[rest[c..].find("*/").unwrap() + c + 2..];
    }
    out.extend(numbers(rest));
    out
}

fn numbers(text: &str) -> Vec<u32> {
    text.split(|c: char| c == ',' || c == '{' || c == '}' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|t| match t.strip_prefix("0x") {
            Some(hex) => u32::from_str_radix(hex, 16).unwrap(),
            None => t.parse().unwrap(),
        })
        .collect()
}

#[test]
fn both_c_decoders_read_every_delta_coded_cutoff_as_tablegen_wrote_it() {
    let header =
        std::fs::read_to_string(sidou_tablegen::repo_root().join("core/sid/gen/filter.h")).unwrap();
    let mut entries = 0;
    for (name, c_model) in [("6581", SID_MODEL_6581), ("8580", SID_MODEL_8580)] {
        let to16 = |v: Vec<u32>| -> Vec<u16> { v.into_iter().map(|x| x as u16).collect() };
        let at = to16(c_array_numbers(&header, &format!("SID_CUTOFF_AT_{name}")));
        let run: Vec<u8> = c_array_numbers(&header, &format!("SID_CUTOFF_RUN_{name}"))
            .into_iter()
            .map(|x| u8::try_from(x).unwrap())
            .collect();
        let curves = unsafe { sid_filter_curves(c_model) } as usize;
        assert_eq!(at.len(), curves * 2048 / sidou_tablegen::CUTOFF_BLOCK);
        let hz = |code: u16| f32::from_bits((u32::from(code) << 11) + (126 << 23));
        for curve in 0..curves {
            let want: Vec<u16> = (0..2048)
                .map(|fc| sidou_tablegen::cutoff_code_at(&at, &run, curve, fc))
                .collect();
            let mut seq = sid_cutoff_seq::default();
            unsafe { sid_cutoff_seq_init(&mut seq, c_model, curve as u32) };
            for fc in 0..2048u32 {
                let w = hz(want[fc as usize]).to_bits();
                assert_eq!(
                    unsafe { sid_cutoff_hz(c_model, curve as u32, fc) }.to_bits(),
                    w,
                    "{name} curve {curve} FC {fc}"
                );
                assert_eq!(
                    unsafe { sid_cutoff_seq_hz(&mut seq, fc) }.to_bits(),
                    w,
                    "{name} curve {curve} FC {fc}, in order"
                );
                entries += 1;
            }
            let mut seq = sid_cutoff_seq::default();
            unsafe { sid_cutoff_seq_init(&mut seq, c_model, curve as u32) };
            let gaps_repeats_and_steps_back = [
                5u32,
                5,
                6,
                40,
                31,
                32,
                33,
                2047,
                0,
                1,
                64,
                63,
                1000,
                1001,
                2048 + 7,
            ];
            for fc in gaps_repeats_and_steps_back {
                assert_eq!(
                    unsafe { sid_cutoff_seq_hz(&mut seq, fc) }.to_bits(),
                    hz(want[(fc & 2047) as usize]).to_bits(),
                    "{name} curve {curve} FC {fc}, out of order"
                );
            }
        }
    }
    assert_eq!(entries, 14_336);
}

const MAX_RELATIVE_ERROR: f64 = 2.5e-4;

#[test]
fn every_cutoff_code_decodes_within_the_relative_error_bound() {
    let header =
        std::fs::read_to_string(sidou_tablegen::repo_root().join("core/sid/gen/filter.h")).unwrap();
    for model in ["6581", "8580"] {
        assert!(header.contains(&format!("static const uint8_t SID_CUTOFF_RUN_{model}")));
    }
    let fits = sidou_tablegen::filter_fits();
    let mut worst = 0.0f64;
    let mut entries = 0;
    for (model, c_model) in [
        (Model::Mos6581, SID_MODEL_6581),
        (Model::Csg8580, SID_MODEL_8580),
    ] {
        let rows: Vec<_> = fits.iter().filter(|f| f.setting.model == model).collect();
        assert_eq!(rows.len(), unsafe { sid_filter_curves(c_model) } as usize);
        for (curve, fit) in rows.iter().enumerate() {
            assert_eq!(fit.cutoff_hz.len(), 2048);
            for (fc, hz) in fit.cutoff_hz.iter().enumerate() {
                let original = format!("{hz:.1}").parse::<f32>().unwrap().max(0.5);
                let decoded = unsafe { sid_cutoff_hz(c_model, curve as u32, fc as u32) };
                let error = (f64::from(decoded) / f64::from(original) - 1.0).abs();
                assert!(
                    error <= MAX_RELATIVE_ERROR,
                    "{model:?} curve {curve} FC {fc}: {original} -> {decoded}"
                );
                let code = (original.to_bits() - (126 << 23) + (1 << 10)) >> 11;
                assert!(u16::try_from(code).is_ok());
                assert_eq!(decoded.to_bits(), (code << 11) + (126 << 23));
                worst = worst.max(error);
                entries += 1;
            }
        }
    }
    println!("{entries} cutoff codes; worst relative error {worst:.9e}");
}
