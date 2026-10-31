// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use gt2_ref::trace::trace;
use gt2_ref::*;

fn number(flag: &str, s: &str, radix: u32, max: u32) -> Result<u32, String> {
    let bad = || {
        let kind = if radix == 16 { "hex" } else { "decimal" };
        format!("{flag}: {s:?} is not a {kind} number up to {max}")
    };
    if s.is_empty() || !s.chars().all(|c| c.is_digit(radix)) {
        return Err(bad());
    }
    match u32::from_str_radix(s, radix) {
        Ok(v) if v <= max => Ok(v),
        _ => Err(bad()),
    }
}

fn flag(o: &mut Options, name: &str, bit: u32, v: &str) -> Result<(), String> {
    if number(name, v, 10, 1)? != 0 {
        o.player_version |= bit;
    } else {
        o.player_version &= !bit;
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut song = None;
    let (mut subtune, mut frames) = (0u8, 3000usize);
    let mut o = Options::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--subtune" || a == "--frames" {
            let v = args.get(i + 1).ok_or(format!("{a} needs a value"))?;
            if a == "--subtune" {
                subtune = number(a, v, 10, 31)? as u8;
            } else {
                frames = number(a, v, 10, u32::MAX)? as usize;
            }
            i += 2;
            continue;
        }
        if a.starts_with("--") || a == "-" {
            return Err(format!("unknown option {a}"));
        }
        if let Some(opt) = a.strip_prefix('-') {
            let k = opt.chars().next().unwrap().to_ascii_uppercase();
            let v = &opt[1..];
            let none = || {
                if v.is_empty() {
                    Ok(())
                } else {
                    Err(format!("{a}: -{k} takes no value"))
                }
            };
            match k {
                'A' => o.hr_adsr = number(a, v, 16, 0xffff)?,
                'S' => o.multiplier = number(a, v, 10, 16)?,
                'B' => flag(&mut o, a, PLAYER_BUFFERED, v)?,
                'C' => flag(&mut o, a, PLAYER_ZPGHOSTREGS, v)?,
                'D' => flag(&mut o, a, PLAYER_SOUNDEFFECTS, v)?,
                'E' => flag(&mut o, a, PLAYER_VOLUME, v)?,
                'H' => flag(&mut o, a, PLAYER_AUTHORINFO, v)?,
                'I' => flag(
                    &mut o,
                    a,
                    PLAYER_NOOPTIMIZATION,
                    if number(a, v, 10, 1)? != 0 { "0" } else { "1" },
                )?,
                'J' => flag(&mut o, a, PLAYER_FULLBUFFERED, v)?,
                'G' => {
                    o.a4_hz = match v.parse::<f32>() {
                        Ok(hz) if hz.is_finite() && hz >= 0.0 => hz,
                        _ => return Err(format!("{a}: {v:?} is not a pitch in Hz")),
                    }
                }
                'L' => o.sid_address = number(a, v, 16, 0xffff)?,
                'N' => {
                    none()?;
                    o.ntsc = 1
                }
                'P' => {
                    none()?;
                    o.ntsc = 0
                }
                'O' => o.optimize_pulse = number(a, v, 10, 1)?,
                'R' => o.optimize_realtime = number(a, v, 10, 1)?,
                'V' => o.finevibrato = number(a, v, 10, 1)?,
                'W' => o.player_address = number(a, v, 16, 0xff)? << 8,
                'Z' => o.zeropage = number(a, v, 16, 0xff)?,
                _ => return Err(format!("unknown option {a}")),
            }
        } else if song.replace(a.clone()).is_some() {
            return Err(format!("one song only: {a}"));
        }
        i += 1;
    }
    let path = song.ok_or("usage: gt2-trace song.sng [--subtune N] [--frames N] [options]")?;
    let sng = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let t = trace(&sng, subtune, frames, &o).map_err(|e| e.to_string())?;
    let line = |w: &[(u8, u8)]| {
        w.iter()
            .map(|(r, v)| format!("{r:02X}={v:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!("init: {}", line(&t.init));
    for (n, f) in t.frames.iter().enumerate() {
        for (c, w) in f.calls.iter().enumerate() {
            if f.calls.len() > 1 {
                println!("{n} .{c}: {}", line(w));
            } else {
                println!("{n}: {}", line(w));
            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("gt2-trace: {e}");
        std::process::exit(1);
    }
}
