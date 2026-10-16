// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `gt2-trace song.sng [--subtune N] [--frames N] [gt2reloc options]`: prints each frame's
//! SID writes from GoatTracker's packed player, for debugging SIDOU's player against it.
//! The options are gt2reloc's (readme 4.4): `-Axxxx -Sn -Bn -Cn -Dn -En -Gf -Hn -In -Jn
//! -Lxxxx -N -On -P -Rn -Vn -Wxx -Zxx`.

use gt2_ref::trace::trace;
use gt2_ref::*;

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

fn dec(s: &str) -> Result<u32, String> {
    s.parse().map_err(|e| format!("{s}: {e}"))
}

fn flag(o: &mut Options, bit: u32, v: &str) -> Result<(), String> {
    if dec(v)? != 0 {
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
        match a.as_str() {
            "--subtune" | "--frames" => {
                let v = args.get(i + 1).ok_or(format!("{a} needs a value"))?;
                if a == "--subtune" {
                    subtune = dec(v)? as u8;
                } else {
                    frames = dec(v)? as usize;
                }
                i += 2;
                continue;
            }
            _ => {}
        }
        if let Some(opt) = a.strip_prefix('-') {
            let (k, v) = opt.split_at(1.min(opt.len()));
            match k.to_ascii_uppercase().as_str() {
                "A" => o.hr_adsr = hex(v)?,
                "S" => o.multiplier = dec(v)?,
                "B" => flag(&mut o, PLAYER_BUFFERED, v)?,
                "C" => flag(&mut o, PLAYER_ZPGHOSTREGS, v)?,
                "D" => flag(&mut o, PLAYER_SOUNDEFFECTS, v)?,
                "E" => flag(&mut o, PLAYER_VOLUME, v)?,
                "H" => flag(&mut o, PLAYER_AUTHORINFO, v)?,
                "I" => {
                    // -I1: optimisations on (gt2reloc's -I also falls through into -J;
                    // here it does not)
                    flag(
                        &mut o,
                        PLAYER_NOOPTIMIZATION,
                        if dec(v)? != 0 { "0" } else { "1" },
                    )?
                }
                "J" => flag(&mut o, PLAYER_FULLBUFFERED, v)?,
                "G" => o.a4_hz = v.parse().map_err(|e| format!("{v}: {e}"))?,
                "L" => o.sid_address = hex(v)?,
                "N" => o.ntsc = 1,
                "P" => o.ntsc = 0,
                "O" => o.optimize_pulse = dec(v)?,
                "R" => o.optimize_realtime = dec(v)?,
                "V" => o.finevibrato = dec(v)?,
                "W" => o.player_address = hex(v)? << 8,
                "Z" => o.zeropage = hex(v)?,
                _ => return Err(format!("unknown option {a}")),
            }
        } else {
            song = Some(a.clone());
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
