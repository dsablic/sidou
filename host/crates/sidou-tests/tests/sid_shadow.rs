// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_void;

use residfp_sys::{CombinedWaveforms, Model, Sampling, Sid};
use sidou_sys::*;
use sidou_tests::psid_fixtures::{Psid, CLOCK_PAL};
use sidou_tests::render::{device_sid, is_voice_control, render_then_advance_to};
use sidou_tests::song::Rng;

const RATE: u32 = 44_118;
const MODELS: [sid_model; 2] = [SID_MODEL_6581, SID_MODEL_8580];
const PROFILES: [(i32, CombinedWaveforms); 3] = [
    (SID_PROFILE_AVERAGE as i32, CombinedWaveforms::Average),
    (SID_PROFILE_WEAK as i32, CombinedWaveforms::Weak),
    (SID_PROFILE_STRONG as i32, CombinedWaveforms::Strong),
];
const SYNC: u8 = 0x02;
const TEST: u8 = 0x08;
const VOICE3_FIRST_REG: u8 = 0x0e;
const OSC3: u8 = 0x1b;
const ENV3: u8 = 0x1c;

fn ttl(model: sid_model) -> u32 {
    if model == SID_MODEL_6581 {
        SID_SHADOW_TTL_6581
    } else {
        SID_SHADOW_TTL_8580
    }
}

fn shadow_of(chip: &sid, now: u32) -> Box<sid_shadow> {
    let mut sh = Box::<sid_shadow>::default();
    unsafe { sid_shadow_init(&mut *sh, chip, now) };
    sh
}

fn sh_write(sh: &mut sid_shadow, x: u32, reg: u8, value: u8) {
    unsafe { sid_shadow_write(sh, x, u32::from(reg), u32::from(value)) }
}

fn sh_read(sh: &mut sid_shadow, x: u32, reg: u8) -> u8 {
    let v = unsafe { sid_shadow_read(sh, x, u32::from(reg)) };
    u8::try_from(v).expect("a byte")
}

fn residfp(model: sid_model, cws: CombinedWaveforms) -> Sid {
    let mut r = Sid::new();
    r.set_chip_model(if model == SID_MODEL_6581 {
        Model::Mos6581
    } else {
        Model::Csg8580
    })
    .unwrap();
    r.set_sampling_parameters(
        f64::from(SID_CLOCK_PAL),
        Sampling::Decimate,
        f64::from(RATE),
    )
    .unwrap();
    r.reset();
    r.set_combined_waveforms(cws);
    r
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Gap {
    FloatingDac,
    AfterWrite,
    OnTheBus,
}

#[derive(Clone, Copy, Debug)]
struct Access {
    at: u32,
    reg: u8,
    value: Option<u8>,
}

fn random_accesses(rng: &mut Rng, n: usize, lifetime: u32, sync: bool) -> Vec<Access> {
    let mut at = 1u32;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        at += match rng.below(32) {
            0 if rng.below(8) == 0 => lifetime - 8 + rng.below(16),
            0..=3 => rng.below(20_000),
            4..=9 => 0,
            10..=19 => rng.below(4),
            _ => rng.below(300),
        };
        if rng.chance(45) {
            let reg = if rng.chance(50) {
                VOICE3_FIRST_REG + rng.below(7) as u8
            } else {
                rng.below(0x20) as u8
            };
            let mut value = rng.byte();
            if is_voice_control(reg) {
                if !sync {
                    value &= !SYNC;
                }
                if rng.below(4) != 0 {
                    value &= !TEST;
                }
            }
            out.push(Access {
                at,
                reg,
                value: Some(value),
            });
        } else {
            let reg = match rng.below(8) {
                0..=2 => OSC3,
                3..=4 => ENV3,
                _ => rng.below(0x20) as u8,
            };
            out.push(Access {
                at,
                reg,
                value: None,
            });
        }
    }
    out
}

#[derive(Default)]
struct Compared {
    reads: usize,
    gaps: std::collections::BTreeMap<Gap, (usize, usize)>,
    wrong: Vec<String>,
    osc3: Vec<(usize, u8, u8)>,
    waveforms: std::collections::BTreeSet<u8>,
    env3: std::collections::BTreeSet<u8>,
}

fn against_residfp(
    model: sid_model,
    profile: (i32, CombinedWaveforms),
    list: &[Access],
    mut gap: impl FnMut(&sid_shadow, &[u32; 0x20], u32) -> Option<Gap>,
) -> Compared {
    let mut chip = device_sid(model);
    unsafe { sid_set_combined_profile(&mut *chip, profile.0) };
    let mut sh = shadow_of(&chip, 0);
    let mut r = residfp(model, profile.1);
    let mut now = 0u32;
    let mut written = [u32::MAX / 2; 0x20];
    let mut bus_gap: Option<Gap> = None;
    let mut latched_gap: Option<Gap> = None;
    let mut out = Compared::default();
    for (i, a) in list.iter().enumerate() {
        if a.at > now {
            r.clock_silent(a.at - now);
            now = a.at;
        }
        match a.value {
            Some(v) => {
                r.write(a.reg, v);
                unsafe { sid_shadow_clock(&mut *sh, a.at) };
                if (0x07..=0x12).contains(&a.reg) && sh.vf.osc3_held == 0 {
                    latched_gap = gap(&sh, &written, a.at);
                }
                sh_write(&mut sh, a.at, a.reg, v);
                written[a.reg as usize] = a.at;
                bus_gap = None;
            }
            None => {
                let theirs = r.read(a.reg);
                let ours = sh_read(&mut sh, a.at, a.reg);
                out.reads += 1;
                let g = match a.reg {
                    OSC3 if sh.vf.osc3_held != 0 => latched_gap,
                    OSC3 => gap(&sh, &written, a.at),
                    0x19 | 0x1a | ENV3 => None,
                    _ => bus_gap,
                };
                if (0x19..=0x1c).contains(&a.reg) {
                    bus_gap = (a.reg == OSC3 && ours != theirs).then_some(Gap::OnTheBus);
                }
                if a.reg == OSC3 && g.is_none() {
                    out.osc3.push((i, ours, theirs));
                    out.waveforms.insert(sh.osc[2].control >> 4);
                }
                if a.reg == ENV3 {
                    out.env3.insert(theirs);
                }
                match g {
                    Some(g) => {
                        let e = out.gaps.entry(g).or_default();
                        e.0 += 1;
                        e.1 += usize::from(ours != theirs);
                    }
                    None if ours != theirs => out.wrong.push(format!(
                        "access {i} at cycle {}: ${:02X} reads {ours:#04x}, reSIDfp {theirs:#04x}",
                        a.at, a.reg
                    )),
                    None => {}
                }
            }
        }
    }
    out
}

fn gap_without_hard_sync(sh: &sid_shadow, written: &[u32; 0x20], at: u32) -> Option<Gap> {
    if sh.osc[2].control >> 4 == 0 {
        return Some(Gap::FloatingDac);
    }
    let recent = |reg: usize| (1..=2).contains(&at.wrapping_sub(written[reg]));
    if [0x07, 0x08, 0x0e, 0x0f, 0x10, 0x11, 0x12]
        .iter()
        .any(|&r| recent(r))
    {
        return Some(Gap::AfterWrite);
    }
    None
}

fn report(what: &str, c: &Compared) {
    let gaps: Vec<String> = c
        .gaps
        .iter()
        .map(|(g, (n, d))| format!("{g:?} {n} reads, {d} differ"))
        .collect();
    println!("{what}: {} reads; skipped: {}", c.reads, gaps.join(", "));
}

#[test]
fn every_read_is_residfps_but_the_listed_gaps() {
    let n = if cfg!(debug_assertions) {
        3_000
    } else {
        40_000
    };
    let mut failures = Vec::new();
    for model in MODELS {
        for profile in PROFILES {
            let mut rng = Rng(0x5eed_0000 + u64::from(model) * 16 + profile.0 as u64);
            let list = random_accesses(&mut rng, n, ttl(model), false);
            let c = against_residfp(model, profile, &list, gap_without_hard_sync);
            report(&format!("model {model} profile {:?}", profile.1), &c);
            assert!(c.reads > n / 2);
            assert!(
                (1..16).all(|w| c.waveforms.contains(&w)),
                "{:?}",
                c.waveforms
            );
            let env3 = if cfg!(debug_assertions) { 80 } else { 200 };
            assert!(
                c.env3.len() > env3,
                "model {model}: {} ENV3 values",
                c.env3.len()
            );
            failures.extend(
                c.wrong
                    .iter()
                    .take(5)
                    .map(|w| format!("model {model} {:?}: {w}", profile.1)),
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn rendering_chip_osc3(model: sid_model, profile: i32, list: &[Access]) -> Vec<Option<u8>> {
    let mut chip = device_sid(model);
    unsafe { sid_set_combined_profile(&mut *chip, profile) };
    let mut clocked = 0u64;
    list.iter()
        .map(|a| {
            let x = u64::from(a.at);
            render_then_advance_to(&mut chip, &mut clocked, x);
            match a.value {
                Some(v) => {
                    unsafe { sid_write(&mut *chip, a.reg, v) };
                    None
                }
                None if a.reg == OSC3 => Some(unsafe { sid_read(&mut *chip, OSC3) }),
                None => None,
            }
        })
        .collect()
}

#[test]
fn with_hard_sync_the_shadow_reads_as_well_as_the_audio_chip() {
    let n = if cfg!(debug_assertions) {
        1_500
    } else {
        20_000
    };
    let mut failures = Vec::new();
    for model in MODELS {
        for seed in 0..if cfg!(debug_assertions) { 1 } else { 4 } {
            let profile = PROFILES[0];
            let mut rng = Rng(0x5c_0000 + u64::from(model) * 16 + seed);
            let list = random_accesses(&mut rng, n, ttl(model), true);
            let c = against_residfp(model, profile, &list, gap_without_hard_sync);
            let chip = rendering_chip_osc3(model, profile.0, &list);
            let shadow_off = c.osc3.iter().filter(|r| r.1 != r.2).count();
            let chip_off = c.osc3.iter().filter(|r| chip[r.0] != Some(r.2)).count();
            report(&format!("hard sync, model {model}, seed {seed}"), &c);
            println!(
                "  OSC3 reads outside the gaps: {}; not reSIDfp's: the shadow {shadow_off}, the \
                 audio chip {chip_off}",
                c.osc3.len()
            );
            let other: Vec<&String> = c.wrong.iter().filter(|w| !w.contains("$1B")).collect();
            if !other.is_empty() {
                failures.push(format!("model {model} seed {seed}: {}", other[0]));
            }
            if shadow_off > chip_off {
                failures.push(format!(
                    "model {model} seed {seed}: the shadow misses {shadow_off} OSC3 reads, the \
                     audio chip {chip_off}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_bus_fades_after_residfps_lifetime_and_a_read_halves_it() {
    for model in MODELS {
        let t = ttl(model);
        let chip = device_sid(model);
        let mut sh = shadow_of(&chip, 0);
        assert_eq!(sh_read(&mut sh, 10, 0x00), 0);
        sh_write(&mut sh, 100, 0x05, 0x5a);
        assert_eq!(sh_read(&mut sh, 100 + t - 1, 0x1f), 0x5a, "model {model}");
        sh_write(&mut sh, 100 + t, 0x05, 0x5b);
        assert_eq!(sh_read(&mut sh, 100 + 2 * t, 0x05), 0, "model {model}");
        let b = 3 * t;
        sh_write(&mut sh, b, 0x15, 0x33);
        assert_eq!(
            sh_read(&mut sh, b, 0x00),
            0x33,
            "a read of a write-only register halves what is left"
        );
        assert_eq!(sh_read(&mut sh, b + t / 2 - 1, 0x00), 0x33);
        assert_eq!(sh_read(&mut sh, b + 4 * t, 0x00), 0x33, "model {model}");
        assert_eq!(sh_read(&mut sh, b + 4 * t, 0x19), 0xff);
        unsafe { sid_shadow_clock(&mut *sh, b + 5 * t - 1) };
        assert_eq!(sh.bus, 0xff);
        assert_eq!(sh_read(&mut sh, b + 5 * t, 0x00), 0, "model {model}");
        let mut r = residfp(model, CombinedWaveforms::Average);
        r.write(0x05, 0x5a);
        r.clock_silent(t - 1);
        assert_eq!(r.read(0x1f), 0x5a);
        r.write(0x05, 0x5b);
        r.clock_silent(t);
        assert_eq!(r.read(0x05), 0);
    }
}

#[test]
fn the_paddles_read_ff_and_the_filter_registers_only_reach_the_bus() {
    for model in MODELS {
        let chip = device_sid(model);
        let mut sh = shadow_of(&chip, 0);
        let mut plain = shadow_of(&chip, 0);
        for (i, (reg, v)) in [(0x15, 0x07), (0x16, 0xff), (0x17, 0xf7), (0x18, 0x9f)]
            .into_iter()
            .enumerate()
        {
            sh_write(&mut sh, 50 + i as u32, reg, v);
            assert_eq!(sh_read(&mut sh, 51 + i as u32, 0x1e), v);
        }
        assert_eq!(sh_read(&mut sh, 60, 0x19), 0xff);
        assert_eq!(sh_read(&mut sh, 60, 0x1a), 0xff);
        sh_read(&mut plain, 60, 0x19);
        assert_eq!(shadow_voices(&sh), shadow_voices(&plain), "model {model}");
    }
}

fn settled_voices(osc: &[sid_osc; 3], env: &[sid_env; 3]) -> Vec<u32> {
    let mut v = Vec::new();
    for i in 0..3 {
        let mut o = osc[i];
        unsafe { sid_noise_settle(&mut o) };
        let e = &env[i];
        v.extend([
            o.acc,
            o.freq,
            o.pw,
            u32::from(o.control),
            o.out,
            o.sr,
            o.noise,
        ]);
        v.extend([e.counter, e.env3, e.gate, e.attack, e.decay, e.sustain].map(u32::from));
        v.push(e.state);
    }
    v
}

fn voices_and_flags(osc: &[sid_osc; 3], env: &[sid_env; 3], vf: &sid_vflags) -> Vec<u32> {
    let mut v = settled_voices(osc, env);
    v.extend([vf.osc3_held, vf.osc3_latch, vf.control_age].map(u32::from));
    v
}

fn shadow_voices(sh: &sid_shadow) -> Vec<u32> {
    voices_and_flags(&sh.osc, &sh.env, &sh.vf)
}

fn chip_voices(s: &sid) -> Vec<u32> {
    voices_and_flags(&s.osc, &s.env, &s.vf)
}

#[test]
fn a_shadow_started_from_a_playing_chip_follows_it_write_for_write() {
    for model in MODELS {
        let mut rng = Rng(0xab_cdef + u64::from(model));
        let mut chip = device_sid(model);
        let warm = random_accesses(&mut rng, 300, ttl(model), false);
        let mut now = 0u32;
        for a in &warm {
            unsafe { sid_clock(&mut *chip, a.at - now) };
            now = a.at;
            if let Some(v) = a.value {
                unsafe { sid_write(&mut *chip, a.reg, v) };
            }
        }
        let start = now.wrapping_add(0xffff_0000);
        let mut sh = shadow_of(&chip, start);
        assert_eq!(chip_voices(&chip), shadow_voices(&sh));
        let base = now;
        for (i, a) in random_accesses(&mut rng, 3_000, ttl(model), false)
            .iter()
            .enumerate()
        {
            let at = base + a.at;
            unsafe { sid_clock(&mut *chip, at - now) };
            now = at;
            let x = start.wrapping_add(a.at);
            match a.value {
                Some(v) => {
                    unsafe { sid_write(&mut *chip, a.reg, v) };
                    sh_write(&mut sh, x, a.reg, v);
                }
                None if a.reg == OSC3 || a.reg == ENV3 => {
                    let theirs = unsafe { sid_read(&mut *chip, a.reg) };
                    assert_eq!(
                        sh_read(&mut sh, x, a.reg),
                        theirs,
                        "model {model}, access {i}"
                    );
                }
                None => {
                    sh_read(&mut sh, x, a.reg);
                }
            }
            assert_eq!(
                chip_voices(&chip),
                shadow_voices(&sh),
                "model {model}, access {i}"
            );
        }
    }
}

#[test]
fn an_access_at_a_passed_cycle_clocks_nothing() {
    let chip = device_sid(SID_MODEL_6581);
    let mut sh = shadow_of(&chip, 1000);
    sh_write(&mut sh, 1000, 0x12, 0x21);
    sh_write(&mut sh, 1000, 0x0f, 0x40);
    unsafe { sid_shadow_clock(&mut *sh, 5000) };
    let at = shadow_voices(&sh);
    assert_eq!(sh.now, 5000);
    sh_read(&mut sh, 4000, ENV3);
    unsafe { sid_shadow_clock(&mut *sh, 4999) };
    assert_eq!(shadow_voices(&sh), at);
    assert_eq!(sh.now, 5000);
}

#[test]
fn a_chips_own_noise_write_back_table_is_copied_not_shared() {
    let mut chip = device_sid(SID_MODEL_6581);
    let own = std::ptr::addr_of!(chip.noise_wb).cast::<[u32; 16]>();
    for v in 0..3 {
        chip.osc[v].writeback = own;
        chip.osc[v].writeback_keep = 0x10;
    }
    for i in 0..16 {
        chip.noise_wb[0][i] = 0x1234 + i as u32;
        chip.noise_wb[1][i] = 0x40 * i as u32;
    }
    let sh = shadow_of(&chip, 0);
    assert_eq!(sh.noise_wb, chip.noise_wb);
    for v in 0..3 {
        assert_eq!(sh.osc[v].writeback, std::ptr::addr_of!(sh.noise_wb).cast());
    }
    let plain = device_sid(SID_MODEL_6581);
    let sh = shadow_of(&plain, 0);
    assert_eq!(sh.osc[0].writeback, plain.osc[0].writeback);
}

type Stamped = Vec<(u32, u8, u8)>;

unsafe extern "C" fn collect_stamped_write(
    ctx: *mut c_void,
    cycle: u32,
    _chip: u32,
    reg: u32,
    value: u32,
) {
    let w = unsafe { &mut *(ctx as *mut Stamped) };
    w.push((cycle, reg as u8, value as u8));
}

fn psid_env_write_stream(song: &[u8], frames: u32) -> Stamped {
    let o = gt2_ref::Options::default();
    let packed = gt2_ref::pack(song, &o).expect("pack");
    let setup = gt2_ref::trace::Setup::of(&packed, &o);
    let p = Psid {
        version: 2,
        load: packed.load,
        init: setup.init,
        play: setup.play,
        songs: 1,
        start: 1,
        speed: 0,
        flags: CLOCK_PAL,
        sid2: 0,
        sid3: 0,
        data: packed.code.clone(),
    };
    let file = p.bytes();
    let mut header = Box::<psid_header>::default();
    let r = unsafe { psid_parse(file.as_ptr(), file.len() as u32, &mut *header) };
    assert_eq!(r, PSID_OK as i32);
    let mut env = Box::<psid_env>::default();
    let mut mem = vec![0u8; 0x10000].into_boxed_slice();
    let mut writes: Box<Stamped> = Box::default();
    unsafe {
        psid_env_load(&mut *env, mem.as_mut_ptr(), &*header, file.as_ptr());
        psid_env_set_sink(
            &mut *env,
            Some(collect_stamped_write),
            (&mut *writes as *mut Stamped).cast(),
        );
        assert_eq!(psid_env_start(&mut *env, 0), 0);
    }
    let end = frames * PSID_FRAME_PAL;
    while env.cpu.cycles < end {
        let code = unsafe { psid_env_run(&mut *env, 5_000) } as u32;
        assert!(
            [
                PSID_RUN_MORE,
                PSID_RUN_IDLE,
                PSID_RUN_INIT_DONE,
                PSID_RUN_CALL_DONE
            ]
            .contains(&code),
            "the tune stopped: {code}"
        );
    }
    *writes
}

#[test]
fn the_shadow_follows_a_rendering_chip_through_the_goattracker_examples() {
    let frames = if cfg!(debug_assertions) { 50 } else { 500 };
    let mut synced = Vec::new();
    for (name, song) in gt2_ref::examples() {
        let writes = psid_env_write_stream(&song, frames);
        assert!(!writes.is_empty(), "{name}");
        let uses_sync = writes
            .iter()
            .any(|&(_, reg, v)| is_voice_control(reg) && v & SYNC != 0);
        for model in MODELS {
            let mut chip = device_sid(model);
            let mut sh = shadow_of(&chip, 0);
            let mut clocked = 0u64;
            let mut first_difference = None;
            for (i, &(x, reg, value)) in writes.iter().enumerate() {
                render_then_advance_to(&mut chip, &mut clocked, u64::from(x));
                unsafe { sid_shadow_clock(&mut *sh, x) };
                let reads = |c: &mut sid, s: &mut sid_shadow| {
                    let theirs = unsafe { [sid_read(c, OSC3), sid_read(c, ENV3)] };
                    let ours = [sh_read(s, x, OSC3), sh_read(s, x, ENV3)];
                    (ours, theirs)
                };
                let (ours, theirs) = reads(&mut chip, &mut sh);
                if (chip_voices(&chip) != shadow_voices(&sh) || ours != theirs)
                    && first_difference.is_none()
                {
                    first_difference = Some(i);
                }
                unsafe { sid_write(&mut *chip, reg, value) };
                sh_write(&mut sh, x, reg, value);
            }
            match first_difference {
                None => {}
                Some(i) if uses_sync => synced.push(format!(
                    "{name}, model {model}: from write {i} of {}",
                    writes.len()
                )),
                Some(i) => panic!(
                    "{name}, model {model}: the shadow differs before write {i} at cycle {}",
                    writes[i].0
                ),
            }
        }
    }
    println!(
        "hard sync's stepping difference (the songs that set SYNC): {}",
        if synced.is_empty() {
            "none".to_string()
        } else {
            synced.join("; ")
        }
    );
}

#[test]
fn the_shadow_fits_its_share_of_the_overlay() {
    assert_eq!(ilp32::sid_shadow::SIZE, 704);
}
