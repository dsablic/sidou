// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use sidou_sys::*;

const RATE: u32 = 44_118;

unsafe extern "C" fn drain_now(ctx: *mut c_void) {
    let o = unsafe { &mut *ctx.cast::<reg_ring_out>() };
    unsafe { reg_ring_drain(o.ring, o.chips) };
}

unsafe extern "C" fn moved(ctx: *mut c_void, chip: u32) {
    let k = unsafe { &mut *ctx.cast::<Keys>() };
    unsafe { reg_ring_set_keys(&mut *k.chips, chip) };
    k.moves.push(chip);
}

struct Keys {
    ring: Box<reg_ring>,
    chips: Box<reg_ring_chips>,
    outs: [Box<reg_ring_out>; 2],
    kr: Box<keys_route>,
    live: Box<live>,
    moves: Vec<u32>,
    takes: Vec<(u32, u32)>,
}

impl Keys {
    fn new() -> Box<Self> {
        let mut ring = Box::<reg_ring>::default();
        let mut chips = Box::<reg_ring_chips>::default();
        unsafe {
            reg_ring_init(&mut *ring);
            reg_ring_chips_init(&mut *chips, SID_MODEL_6581, SID_CLOCK_PAL, RATE);
        }
        let mut out = |chip: u8| {
            let mut o = Box::new(reg_ring_out {
                ring: &mut *ring,
                chips: &mut *chips,
                chip,
                wait: Some(drain_now),
                ..Default::default()
            });
            let p: *mut reg_ring_out = &mut *o;
            o.wait_ctx = p.cast();
            o
        };
        let outs = [out(0), out(1)];
        let mut k = Box::new(Keys {
            ring,
            chips,
            outs,
            kr: Box::default(),
            live: Box::default(),
            moves: Vec::new(),
            takes: Vec::new(),
        });
        let kp: *mut Keys = &mut *k;
        let a: *mut reg_ring_out = &mut *k.outs[0];
        let b: *mut reg_ring_out = &mut *k.outs[1];
        let krp: *mut keys_route = &mut *k.kr;
        unsafe {
            keys_route_init(
                &mut *k.kr,
                &mut *k.live,
                Some(reg_ring_sink),
                Some(reg_ring_prepare_step),
                a.cast(),
                b.cast(),
            );
            k.kr.moved = Some(moved);
            k.kr.moved_ctx = kp.cast();
            live_init(
                &mut *k.live,
                Some(keys_route_sink),
                krp.cast(),
                SID_MODEL_6581,
            );
            live_set_preparer(&mut *k.live, Some(keys_route_prepare), krp.cast());
            keys_route_start(&mut *k.kr);
        }
        k.block();
        k
    }

    fn block(&mut self) {
        unsafe {
            reg_ring_commit(&mut *self.ring);
            reg_ring_pass_end(&mut *self.chips);
            reg_ring_drain(&mut *self.ring, &mut *self.chips);
        }
    }

    fn tick(&mut self) {
        unsafe {
            live_tick(&mut *self.live);
            keys_route_tick(&mut *self.kr);
        }
        self.block();
    }

    fn ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn chip(&mut self, x: u32) -> &mut sid {
        unsafe { &mut *reg_ring_chip_of(&mut *self.chips, x) }
    }

    fn song(&mut self, on: bool) {
        unsafe { keys_route_set_song(&mut *self.kr, i32::from(on)) };
    }
}

fn render(s: &mut sid, n: usize) -> Vec<i16> {
    let mut out = vec![0i16; n];
    unsafe { sid_render(s, out.as_mut_ptr(), n as u32) };
    out
}

fn gates(s: &sid) -> [u8; 3] {
    s.env.map(|e| e.gate)
}

fn setup(k: &Keys, x: usize) -> (u8, u8, [u8; 5]) {
    let c = &*k.chips;
    let s = &c.chip[usize::from(c.active[x])];
    let n = c.ramp[x].now;
    (
        s.filter_curve,
        s.model as u8,
        [n.profile, n.age, n.dac, n.thump, n.bleed],
    )
}

#[test]
fn a_new_song_clears_user_identity_even_when_its_base_profile_is_unchanged() {
    let mut k = Keys::new();
    k.song(true);
    k.ticks(2);
    k.kr.song_user = 1;
    let mut edit = k.kr.song_want;
    edit.controls = [10, 20, 30, 40];
    unsafe { keys_route_song_wants(&mut *k.kr, &edit) };
    assert_eq!(
        k.kr.song_user, 1,
        "ordinary character edits keep the selection"
    );
    k.song(false);
    k.ticks(2);
    k.song(true);
    k.ticks(2);
    assert_eq!(
        k.kr.song_user, 0,
        "the new song inherits a setup, not a USER selection"
    );
}

#[test]
fn a_refused_explicit_edit_leaves_the_song_setup_and_identity_unchanged() {
    unsafe extern "C" fn refuse(ctx: *mut c_void, _: *const chip_setup, mask: u32) -> i32 {
        assert_eq!(mask, KEYS_ROUTE_EDIT_ALL);
        unsafe { *ctx.cast::<u32>() += 1 };
        0
    }

    let mut k = Keys::new();
    let mut calls = 0u32;
    k.kr.song_user = 2;
    k.kr.song_edit = Some(refuse);
    k.kr.song_edit_ctx = (&mut calls as *mut u32).cast();
    let before = k.kr.song_want;
    let edit = chip_setup {
        profile: SID_CHIP_6581_WORN as u8,
        controls: [10, 20, 30, 40],
        gate_reset: 0,
        expert: [70; SID_EXPERT_COUNT as usize],
    };
    assert_eq!(
        unsafe { keys_route_song_edit(&mut *k.kr, &edit, KEYS_ROUTE_EDIT_ALL) },
        0
    );
    assert_eq!(calls, 1);
    assert_eq!(k.kr.song_want.profile, before.profile);
    assert_eq!(k.kr.song_want.controls, before.controls);
    assert_eq!(k.kr.song_want.gate_reset, before.gate_reset);
    assert_eq!(k.kr.song_want.expert, before.expert);
    assert_eq!(k.kr.song_user, 2);
}

#[test]
fn chip_ctl_follows_a_setup_and_records_what_it_saw() {
    let mut k = Keys::new();
    let want = chip_setup {
        profile: SID_CHIP_6581_WORN as u8,
        controls: [40, 127, 64, 10],
        gate_reset: 0,
        expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
    };
    let mut ctl = Box::<chip_ctl>::default();
    let b: *mut reg_ring_out = &mut *k.outs[1];
    unsafe {
        chip_ctl_init(
            &mut *ctl,
            Some(reg_ring_sink),
            b.cast(),
            Some(reg_ring_prepare_step),
            b.cast(),
        );
    }
    assert_eq!(unsafe { chip_ctl_has(&*ctl, &want) }, 0);
    let mut n = 0;
    while unsafe { chip_ctl_follow(&mut *ctl, &want) } == 0 {
        k.block();
        n += 1;
        assert!(n < 40, "never got there");
    }
    k.block();
    assert_eq!(unsafe { chip_ctl_has(&*ctl, &want) }, 1);
    assert_eq!(k.chip(1).filter_curve, 3, "WORN");
    assert!(k.chip(1).env.iter().all(|e| e.gate_reset == 0));
    assert_eq!(k.outs[1].unprepared_resets, 0, "a reset made at once");
    unsafe { chip_ctl_saw(&mut *ctl, LIVE_REG_CHAR_FIRST as u8 + 2, 3) };
    assert_eq!(ctl.has.controls[1], 3);
    assert_eq!(unsafe { chip_ctl_has(&*ctl, &want) }, 0);
}

#[test]
fn expert_setup_follows_the_keys_without_resetting_a_held_note() {
    let mut k = Keys::new();
    k.ticks(20);
    unsafe {
        live_note_on(&mut *k.live, 60, 100);
        live_param_set(&mut *k.live, LIVE_PARAM_EXPERT + SID_EXPERT_DAC_KINK, 100);
        live_param_set(
            &mut *k.live,
            LIVE_PARAM_EXPERT + SID_EXPERT_FILTER_6581_CURVE,
            90,
        );
    }
    let active = k.chips.active[0];
    k.ticks(1200);
    assert_eq!(
        k.chips.active[0], active,
        "an expert edit must not swap/reset a SID"
    );
    assert_eq!(
        k.chip(0).env[0].gate,
        1,
        "held gate survived the table swaps"
    );
    assert_eq!(
        unsafe {
            chip_ctl_has(&k.kr.ctl[1], &{
                let mut setup = chip_setup::default();
                chip_setup_of(&*k.live, &mut setup);
                setup
            })
        },
        1,
        "chip B must carry the same expert setup"
    );
    for chip in 0..2 {
        let c = &k.chips.ramp[chip].now;
        assert_eq!(
            unsafe { sid_character_expert_get(c, SID_MODEL_6581, SID_EXPERT_DAC_KINK) },
            100
        );
        assert_eq!(
            unsafe { sid_character_expert_get(c, SID_MODEL_6581, SID_EXPERT_FILTER_6581_CURVE) },
            90
        );
    }
    unsafe { live_note_off(&mut *k.live, 60) };
    k.tick();
    assert_eq!(
        k.chip(0).env[0].gate,
        0,
        "the release still reaches the same voice"
    );
}

#[test]
fn chip_b_holds_the_keys_setup_after_every_change() {
    let mut k = Keys::new();
    k.tick();
    assert_eq!(
        u32::from(k.live.other_profile),
        LIVE_PROFILE_NONE,
        "STATUS: chip B not made"
    );
    k.ticks(19);
    assert_eq!(
        u32::from(k.live.other_profile),
        SID_CHIP_6581_R4AR,
        "STATUS: chip B made"
    );
    assert_eq!(k.live.keys_chip, 0);
    assert_eq!(
        setup(&k, 1),
        setup(&k, 0),
        "chip B made with the keys' setup"
    );
    assert_eq!(
        k.outs[1].unprepared_resets, 0,
        "chip B's first reset made at once"
    );
    for change in 0..4 {
        unsafe {
            match change {
                0 => live_param_set(&mut *k.live, LIVE_PARAM_PROFILE, SID_CHIP_6581_WORN),
                1 => live_button(&mut *k.live, LIVE_BUTTON_FX),
                2 => {
                    live_param_set(&mut *k.live, LIVE_PARAM_AGE, 90);
                    live_param_set(&mut *k.live, LIVE_PARAM_DAC, 60);
                }
                _ => live_param_set(
                    &mut *k.live,
                    LIVE_PARAM_ENVELOPE_MODE,
                    LIVE_ENVELOPE_AUTHENTIC,
                ),
            }
        }
        k.ticks(400);
        assert_eq!(setup(&k, 1), setup(&k, 0), "change {change}");
        let gr = |s: &sid| s.env[0].gate_reset;
        let (a, b) = (gr(k.chip(0)), gr(k.chip(1)));
        assert_eq!(a, b, "change {change}: the envelope mode");
    }
    assert!(k.moves.is_empty(), "the keys moved with no song");
    assert_eq!(k.outs[1].unprepared_resets, 0);
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn a_song_moves_the_keys_to_chip_b_and_back_when_no_key_is_held() {
    let mut k = Keys::new();
    k.ticks(20);
    unsafe { live_note_on(&mut *k.live, 60, 100) };
    k.tick();
    assert_eq!(gates(k.chip(0))[0], 1, "the key sounds on A");
    k.song(true);
    k.tick();
    assert_eq!(k.moves, [1]);
    assert_eq!(k.live.keys_chip, 1);
    assert_eq!(gates(k.chip(0))[0], 0, "released on A, the song's now");
    assert_eq!(gates(k.chip(1))[0], 1, "the held key sounds again on B");
    assert!(render(k.chip(1), 4096).iter().any(|&s| s != 0));
    unsafe { live_note_on(&mut *k.live, 64, 100) };
    k.tick();
    assert_eq!(
        gates(k.chip(1)).iter().filter(|&&g| g == 1).count(),
        2,
        "a new note on B"
    );
    assert_eq!(gates(k.chip(0)), [0, 0, 0]);
    k.song(false);
    k.ticks(30);
    assert_eq!(k.live.keys_chip, 1, "moved with keys held");
    unsafe {
        live_note_off(&mut *k.live, 60);
        live_note_off(&mut *k.live, 64);
    }
    k.ticks(2);
    assert_eq!(k.moves, [1, 0], "back to A once no key is held");
    unsafe { live_note_on(&mut *k.live, 67, 100) };
    k.tick();
    assert!(gates(k.chip(0)).contains(&1), "the keys play A again");
    assert_eq!(gates(k.chip(1)), [0, 0, 0], "and not B");
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn during_a_song_the_keys_setup_changes_chip_b_and_a_takes_it_after() {
    let mut k = Keys::new();
    k.ticks(20);
    k.song(true);
    k.tick();
    assert_eq!(k.live.keys_chip, 1);
    let a_before = setup(&k, 0);
    unsafe { live_button(&mut *k.live, LIVE_BUTTON_FX) };
    k.ticks(30);
    assert_eq!(
        k.chip(1).model,
        SID_MODEL_8580,
        "the FX button on the keys' chip"
    );
    assert_eq!(setup(&k, 0), a_before, "the song's chip changed");
    k.song(false);
    let mut back = None;
    for t in 1..=40 {
        k.tick();
        if back.is_none() && k.live.keys_chip == 0 {
            back = Some(t);
            assert_eq!(
                k.chip(0).model,
                SID_MODEL_8580,
                "back before A had the keys' setup"
            );
        }
    }
    let back = back.expect("the keys never went back");
    assert!(
        back > 7,
        "back after {back} ticks: A was not prepared first"
    );
    assert_eq!(k.chips.lost_resets, 0);
}

unsafe extern "C" fn taking(ctx: *mut c_void, voices: u32) {
    let k = unsafe { &mut *ctx.cast::<Keys>() };
    k.takes.push((voices, k.ring.stage));
}

#[test]
fn keys_a_tells_the_song_a_voice_is_taken_before_its_first_register() {
    let mut k = Keys::new();
    let kp: *mut Keys = &mut *k;
    k.kr.taking = Some(taking);
    k.kr.taking_ctx = kp.cast();
    k.ticks(20);
    unsafe { live_note_on(&mut *k.live, 60, 100) };
    k.tick();
    unsafe { live_note_off(&mut *k.live, 60) };
    k.ticks(5);
    assert!(k.takes.is_empty(), "no song: nothing taken");
    unsafe { live_set_keys_mode(&mut *k.live, LIVE_KEYS_A) };
    k.song(true);
    k.tick();
    let before = k.ring.stage;
    unsafe { live_note_on(&mut *k.live, 64, 100) };
    let taken = unsafe { keys_route_voices_taken(&*k.kr) };
    assert_eq!(taken.count_ones(), 1);
    let v = u32::from(taken.trailing_zeros() as u8);
    assert_eq!(k.takes.len(), 1, "one take: {:?}", k.takes);
    let (voices, at) = k.takes[0];
    assert_eq!(voices, u32::from(taken));
    let regs = |from: u32, to: u32| -> Vec<u8> {
        (from..to)
            .map(|i| k.ring.e[(i % REG_RING_SIZE) as usize].reg)
            .filter(|&r| r < 0x40 && u32::from(r) / 7 == v && r < 21)
            .collect()
    };
    assert!(
        regs(before, at).is_empty(),
        "voice {v}'s registers went before the take"
    );
    assert!(
        !regs(at, k.ring.stage).is_empty(),
        "the key's registers follow"
    );
    k.tick();
    unsafe { live_note_off(&mut *k.live, 64) };
    k.ticks(5);
    assert_eq!(k.takes.len(), 1, "the release tells nothing");
    assert_eq!(unsafe { keys_route_voices_taken(&*k.kr) }, 0);
}

#[test]
fn keys_a_takes_a_voice_from_the_song_and_gives_it_back() {
    let mut k = Keys::new();
    k.ticks(20);
    unsafe { live_set_keys_mode(&mut *k.live, LIVE_KEYS_A) };
    k.song(true);
    k.tick();
    assert!(k.moves.is_empty(), "KEYS A moved the keys");
    assert_eq!(unsafe { keys_route_voices_taken(&*k.kr) }, 0);
    unsafe { live_note_on(&mut *k.live, 60, 100) };
    k.tick();
    let taken = unsafe { keys_route_voices_taken(&*k.kr) };
    assert_eq!(taken.count_ones(), 1, "the key's voice: {taken:03b}");
    let v = taken.trailing_zeros() as usize;
    assert_eq!(gates(k.chip(0))[v], 1);
    let (fc, model) = (k.chip(0).fc, k.chip(0).model);
    unsafe {
        live_knob(&mut *k.live, LIVE_KNOB_CUTOFF, -20);
        live_button(&mut *k.live, LIVE_BUTTON_FX);
    }
    k.ticks(20);
    assert_eq!(k.chip(0).fc, fc, "the song's filter changed");
    assert_eq!(k.chip(0).model, model, "the song's chip was reset");
    unsafe { live_note_off(&mut *k.live, 60) };
    k.tick();
    assert_eq!(
        unsafe { keys_route_voices_taken(&*k.kr) },
        0,
        "the voice given back"
    );
    k.song(false);
    k.ticks(30);
    assert_eq!(k.chip(0).model, SID_MODEL_8580);
    let want = u32::from(k.live.cutoff_out);
    assert_eq!(k.chip(0).fc, want);
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn a_key_sounds_the_same_on_either_chip() {
    let play = |song: bool| {
        let mut k = Keys::new();
        k.ticks(20);
        k.song(song);
        k.tick();
        unsafe { live_note_on(&mut *k.live, 57, 100) };
        k.tick();
        let x = u32::from(k.live.keys_chip);
        assert_eq!(x, u32::from(song));
        let s = k.chip(x);
        (
            s.osc[0].freq,
            s.env[0].attack,
            s.env[0].sustain,
            s.filter_curve,
            s.model,
            gates(s),
        )
    };
    assert_eq!(play(false), play(true));
}

#[test]
fn a_move_in_a_heavy_pass_fits_the_ring_without_waiting() {
    let mut k = Keys::new();
    k.ticks(20);
    for n in [48u8, 55, 60] {
        unsafe { live_note_on(&mut *k.live, n, 100) };
    }
    k.block();
    k.song(true);
    unsafe {
        live_set_preset(&mut *k.live, 1);
        for n in 36..63u8 {
            live_note_on(&mut *k.live, n, 100);
            live_note_off(&mut *k.live, n);
        }
        live_tick(&mut *k.live);
        keys_route_tick(&mut *k.kr);
    }
    let used = unsafe { reg_ring_used(&*k.ring) };
    println!("a heavy pass with a move: {used} entries");
    assert_eq!(k.live.keys_chip, 1, "the keys moved");
    assert!(used <= REG_RING_WORST_PASS, "{used} entries");
    assert_eq!(k.outs[0].full_waits + k.outs[1].full_waits, 0);
    k.block();
    assert_eq!(k.chips.lost_resets, 0);
}

fn voice_regs(k: &mut Keys, x: u32, v: usize) -> (u32, u32, u8, u8, u8, u8, u8) {
    let s = k.chip(x);
    let (o, e) = (&s.osc[v], &s.env[v]);
    (
        o.freq, o.pw, o.control, e.attack, e.decay, e.sustain, e.release,
    )
}

fn song_writes(k: &mut Keys) {
    let a: *mut reg_ring_out = &mut *k.outs[0];
    for v in 0..3u8 {
        for (r, val) in [
            (0, 0x34),
            (1, 0x12 + v),
            (2, 0x56),
            (3, 0x07),
            (5, 0x3a),
            (6, 0x9c),
            (4, 0x40),
        ] {
            unsafe { reg_ring_sink(a.cast(), v * 7 + r, val) };
        }
    }
    unsafe { reg_ring_sink(a.cast(), 0x18, 0x1c) };
    k.block();
}

#[test]
fn a_song_before_chip_b_is_ready_waits_and_chip_a_is_already_the_songs() {
    let mut k = Keys::new();
    assert_eq!(k.kr.ctl[1].made, 0, "chip B made at once");
    song_writes(&mut k);
    k.song(true);
    assert_eq!(unsafe { keys_route_ready_for_song(&*k.kr) }, 0);
    let (fc, v2) = (k.chip(0).fc, voice_regs(&mut k, 0, 1));
    unsafe {
        live_knob(&mut *k.live, LIVE_KNOB_CUTOFF, -20);
        live_button(&mut *k.live, LIVE_BUTTON_FX);
        live_param_set(&mut *k.live, LIVE_PARAM_ATTACK, 12);
    }
    k.tick();
    assert_eq!(k.live.keys_chip, 0);
    assert_eq!(
        (k.chip(0).fc, voice_regs(&mut k, 0, 1)),
        (fc, v2),
        "the song's chip changed"
    );
    let mut ready = None;
    for t in 1..40 {
        k.tick();
        if unsafe { keys_route_ready_for_song(&*k.kr) } != 0 {
            ready = Some(t);
            break;
        }
    }
    assert!(ready.is_some(), "never ready for the song");
    assert_eq!(k.live.keys_chip, 1);
    assert_eq!(
        k.chip(1).model,
        SID_MODEL_8580,
        "chip B has the keys' setup (the FX button)"
    );
    unsafe { live_set_keys_mode(&mut *k.live, LIVE_KEYS_A) };
    assert_eq!(
        unsafe { keys_route_ready_for_song(&*k.kr) },
        1,
        "KEYS A is ready at once"
    );
}

#[test]
fn the_songs_setup_on_chip_a_is_taken_back_after_the_song() {
    for mode in [LIVE_KEYS_AUTO, LIVE_KEYS_A] {
        let mut k = Keys::new();
        k.ticks(20);
        unsafe { live_set_keys_mode(&mut *k.live, mode) };
        k.song(true);
        k.ticks(2);
        let song = chip_setup {
            profile: SID_CHIP_6581_R2 as u8,
            controls: [0; 4],
            gate_reset: 1,
            expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
        };
        unsafe { keys_route_song_setup(&mut *k.kr, &song) };
        k.tick();
        if mode == LIVE_KEYS_AUTO {
            assert_eq!(
                u32::from(k.live.other_profile),
                SID_CHIP_6581_R2,
                "STATUS's chip A"
            );
        }
        let slices = k.outs[0].slices;
        k.song(false);
        k.ticks(30);
        assert!(
            k.outs[0].slices > slices,
            "mode {mode}: chip A was not prepared back"
        );
        assert_eq!(k.live.keys_chip, 0, "mode {mode}");
        assert_eq!(
            u32::from(k.kr.ctl[0].has.profile),
            SID_CHIP_6581_R4AR,
            "mode {mode}"
        );
        assert_eq!(k.kr.stale, 0, "mode {mode}");
        assert_eq!(k.chips.lost_resets, 0);
    }
}

#[test]
fn keys_a_leaves_the_songs_other_voices_byte_for_byte() {
    let mut k = Keys::new();
    k.ticks(20);
    k.song(true);
    k.ticks(2);
    assert_eq!(k.live.keys_chip, 1, "AUTO: the keys on chip B");
    song_writes(&mut k);
    let before: Vec<_> = (0..3).map(|v| voice_regs(&mut k, 0, v)).collect();
    unsafe { live_set_keys_mode(&mut *k.live, LIVE_KEYS_A) };
    k.ticks(2);
    assert_eq!(k.live.keys_chip, 0, "KEYS A: back to chip A");
    let all: Vec<_> = (0..3).map(|v| voice_regs(&mut k, 0, v)).collect();
    assert_eq!(all, before, "the move to chip A wrote the song's voices");
    unsafe {
        live_set_preset(&mut *k.live, 1);
        live_note_on(&mut *k.live, 60, 100);
    }
    k.tick();
    let taken = unsafe { keys_route_voices_taken(&*k.kr) };
    assert_eq!(taken.count_ones(), 1);
    let held = taken.trailing_zeros() as usize;
    for t in 0..20 {
        unsafe {
            match t {
                3 => live_param_set(&mut *k.live, LIVE_PARAM_PULSE_WIDTH, 2048 + 9 * 32),
                5 => live_param_set(&mut *k.live, LIVE_PARAM_ATTACK, 3),
                7 => live_param_set(&mut *k.live, LIVE_PARAM_SUSTAIN, 2),
                9 => live_param_set(&mut *k.live, LIVE_PARAM_PULSE_WIDTH, 100),
                _ => {}
            }
        }
        k.tick();
        for v in (0..3).filter(|&v| v != held) {
            assert_eq!(voice_regs(&mut k, 0, v), before[v], "tick {t}: voice {v}");
        }
    }
    assert_eq!(k.chip(0).env[held].gate, 1, "the keys' voice plays");
    unsafe { live_note_off(&mut *k.live, 60) };
    k.tick();
    assert_eq!(k.chip(0).env[held].gate, 0, "its gate-off reached chip A");
    assert_eq!(unsafe { keys_route_voices_taken(&*k.kr) }, 0);
    for v in (0..3).filter(|&v| v != held) {
        assert_eq!(
            voice_regs(&mut k, 0, v),
            before[v],
            "after the release: voice {v}"
        );
    }
    unsafe {
        live_set_preset(&mut *k.live, 2);
        live_note_on(&mut *k.live, 64, 100);
    }
    k.tick();
    assert_eq!(unsafe { keys_route_voices_taken(&*k.kr) }, 0b101);
    assert_eq!(
        voice_regs(&mut k, 0, 1),
        before[1],
        "voice 2 under mono sync"
    );
    let sync = k.live.voice[0].freq_out;
    let _ = sync;
    assert_ne!(
        voice_regs(&mut k, 0, 2).0,
        before[2].0,
        "voice 3, the sync source, is the keys'"
    );
}

#[test]
fn an_edit_of_the_songs_setup_brings_chip_a_to_it() {
    let mut k = Keys::new();
    k.ticks(20);
    k.song(true);
    k.ticks(2);
    assert_eq!(k.live.keys_chip, 1);
    let b_before = setup(&k, 1);
    let song = chip_setup {
        profile: SID_CHIP_6581_WORN as u8,
        controls: [0, 0, 64, 0],
        gate_reset: 1,
        expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
    };
    assert!(k.kr.song_edit.is_none());
    assert!(k.kr.song_edit_ctx.is_null());
    assert_eq!(
        unsafe { keys_route_song_edit(&mut *k.kr, &song, KEYS_ROUTE_EDIT_ALL) },
        1
    );
    k.ticks(40);
    assert_eq!(k.chip(0).filter_curve, 3, "chip A has the song's WORN");
    assert_eq!(k.chips.ramp[0].now.thump, 64, "and its thump");
    assert_eq!(setup(&k, 1), b_before, "the keys' chip changed");
    assert_eq!(k.outs[0].unprepared_resets, 1, "live_init's only");
    assert_eq!(k.chips.lost_resets, 0);
}

fn chip_a_is(k: &Keys, want: &chip_setup) -> bool {
    let p = unsafe { &*sid_chip_profile_get(u32::from(want.profile)) };
    let c = &*k.chips;
    let s = &c.chip[usize::from(c.active[0])];
    let n = c.ramp[0].now;
    s.filter_curve == p.curve
        && s.model as u8 == p.model
        && [n.age, n.dac, n.thump, n.bleed] == want.controls
}

#[test]
fn edits_and_the_songs_chpa_go_through_one_want_path() {
    let mut k = Keys::new();
    k.ticks(20);
    k.song(true);
    k.ticks(2);
    let edit = chip_setup {
        profile: SID_CHIP_6581_WORN as u8,
        controls: [32, 0, 0, 0],
        gate_reset: 1,
        expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
    };
    let chpa = chip_setup {
        profile: SID_CHIP_6581_R2 as u8,
        controls: [0, 0, 64, 0],
        gate_reset: 1,
        expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
    };
    let mut again = chpa;
    again.controls[0] = 48;
    for (what, want) in [
        ("an edit", edit),
        ("the CHPA", chpa),
        ("another edit", again),
    ] {
        unsafe { keys_route_song_wants(&mut *k.kr, &want) };
        k.ticks(200);
        assert!(chip_a_is(&k, &want), "{what}: chip A did not get there");
        let has = k.kr.ctl[0].has;
        assert_eq!(
            (has.profile, has.controls),
            (want.profile, want.controls),
            "{what}: ctl[0]"
        );
    }
    assert_eq!(k.kr.song_want.controls, [48, 0, 64, 0]);
    assert_eq!(k.outs[0].unprepared_resets, 1, "live_init's only");
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn chip_a_comes_back_to_the_keys_setup_after_a_tab_edits() {
    for mode in [LIVE_KEYS_AUTO, LIVE_KEYS_A] {
        let mut k = Keys::new();
        unsafe {
            live_set_keys_mode(&mut *k.live, mode);
            live_param_set(&mut *k.live, LIVE_PARAM_AGE, 20);
        }
        k.ticks(60);
        k.song(true);
        k.ticks(3);
        let edit = chip_setup {
            profile: SID_CHIP_6581_WORN as u8,
            controls: [100, 40, 80, 10],
            gate_reset: 1,
            expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
        };
        unsafe { keys_route_song_wants(&mut *k.kr, &edit) };
        k.ticks(200);
        assert!(
            chip_a_is(&k, &edit),
            "mode {mode}: the edit did not reach chip A"
        );
        let (unprepared, lost) = (k.outs[0].unprepared_resets, k.chips.lost_resets);
        k.song(false);
        k.ticks(200);
        let mut keys = Box::<chip_setup>::default();
        unsafe { chip_setup_of(&*k.live, &mut *keys) };
        assert_eq!(u32::from(keys.profile), SID_CHIP_6581_R4AR);
        assert!(
            chip_a_is(&k, &keys),
            "mode {mode}: chip A kept the song's setup"
        );
        assert_eq!(k.live.keys_chip, 0, "mode {mode}");
        assert_eq!(
            (k.outs[0].unprepared_resets, k.chips.lost_resets),
            (unprepared, lost),
            "mode {mode}"
        );
        assert_eq!(lost, 0);
    }
}

#[test]
fn a_mode_that_gives_chip_a_to_the_player_moves_the_keys_as_a_song_does() {
    let mut k = Keys::new();
    k.ticks(20);
    unsafe { keys_route_set_player(&mut *k.kr, 1) };
    k.tick();
    assert_eq!(k.moves, [1]);
    k.song(true);
    unsafe { keys_route_set_player(&mut *k.kr, 0) };
    k.ticks(30);
    assert_eq!(k.live.keys_chip, 1, "the song still plays");
    k.song(false);
    k.ticks(30);
    assert_eq!(k.moves, [1, 0]);
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn chip_level_ccs_on_chip_as_channels_set_chip_as_setup() {
    let mut k = Keys::new();
    k.ticks(20);
    let mut route = Box::<midi_route>::default();
    let mut s = midi_route_setup::default();
    unsafe {
        midi_route_init(&mut *route, &mut *k.live, &mut *k.kr, std::ptr::null());
        midi_route_setup_default(&mut s);
    }
    s.mode = MIDI_MODE_MULTI as u8;
    unsafe { midi_route_set(&mut *route, &s) };
    k.ticks(2);
    assert_eq!(k.live.keys_chip, 1, "chip A is the player's");
    let cc = |route: &mut midi_route, ch: u8, cc: u8, v: u8| {
        let m = midi_msg {
            kind: MIDI_CC,
            channel: ch,
            data1: cc,
            data2: v,
            bend: 0,
        };
        unsafe { midi_route_take(route, &m) };
    };
    let worn = unsafe { midi_map_to_cc(std::ptr::null(), midi_map_for_cc(85), SID_CHIP_6581_WORN) };
    cc(&mut route, 1, 85, worn);
    cc(&mut route, 2, 89, 127);
    cc(&mut route, 0, 26, 127);
    let want = k.kr.song_want;
    assert_eq!(u32::from(want.profile), SID_CHIP_6581_WORN);
    assert_eq!(want.controls[2], 127);
    assert_eq!(want.gate_reset, 0);
    k.ticks(200);
    assert!(chip_a_is(&k, &want), "chip A did not get there");
    assert_eq!(
        u32::from(k.live.profile),
        SID_CHIP_6581_R4AR,
        "the keys' setup is kept"
    );
    cc(&mut route, 0, 25, 127);
    assert_eq!(u32::from(k.kr.song_want.profile), SID_CHIP_8580_R5);
    cc(&mut route, 0, 25, 127);
    assert_eq!(
        u32::from(k.kr.song_want.profile),
        SID_CHIP_8580_R5,
        "already an 8580"
    );
    s.mode = MIDI_MODE_OMNI as u8;
    unsafe { midi_route_set(&mut *route, &s) };
    k.ticks(60);
    assert_eq!(k.live.keys_chip, 0);
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn chip_edits_under_character_locks_preserve_the_unlocked_song_base() {
    use std::{fs, path::Path, process::Command};

    fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let from = source.find(start).unwrap();
        let to = source[from..].find(end).unwrap() + from;
        &source[from..to]
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let main = fs::read_to_string(root.join("platform/fm1/main.c")).unwrap();
    let start = main.find("static chip_setup song_char_base;").unwrap();
    let end = main[start..].find("static void song_boot(void)").unwrap() + start;
    let chip_ctl = fs::read_to_string(root.join("app/chip_ctl.c")).unwrap();
    let character = fs::read_to_string(root.join("core/sid/sid_character.c")).unwrap();
    let keys_route = fs::read_to_string(root.join("app/keys_route.c")).unwrap();
    let setup_control = between(&chip_ctl, "void chip_setup_control(", "void chip_ctl_init(");
    let character_init = between(
        &character,
        "void sid_character_init(",
        "static const sid_chip_profile* profile_for(",
    );
    let character_edits = between(
        &character,
        "void sid_character_expert_set(",
        "static void dac_params_for(",
    );
    let song_wants = between(
        &keys_route,
        "void keys_route_song_wants(",
        "int keys_route_song_edit(",
    );
    let source = format!(
        r#"
#include <assert.h>
#include <string.h>
#include "keys_route.h"
#include "midi_sync.h"
#include "player/song_ctl.h"
static gt2_song song;
static song_ctl ctl;
static unsigned toasts;
static uint8_t song_away;
static song_ctl* audio_song(void) {{ return &ctl; }}
static void ui_toast(const char* text, uint32_t ms) {{
    assert(strcmp(text, "SAVING") == 0 && ms == 2000u);
    toasts++;
}}
{character_init}
{character_edits}
{setup_control}
{song_wants}
{}
int main(void) {{
    ctl.song = &song;
    keys_route kr = {{0}};
    chip_setup base = {{.profile = 1, .controls = {{10, 20, 30, 40}}, .gate_reset = 1}};
    memset(base.expert, 70, sizeof base.expert);
    keys_route_song_wants(&kr, &base);
    song_chip_character(&kr, 0, 90);
    song_chip_character(&kr, 1, 80);
    assert(memcmp(song_char_base.expert, base.expert, sizeof base.expert) == 0);
    assert(kr.song_want.expert[SID_EXPERT_FILTER_6581_CURVE] == SID_EXPERT_DEFAULT);
    assert(kr.song_want.expert[SID_EXPERT_DAC_KINK] == SID_EXPERT_DEFAULT);
    assert(kr.song_want.expert[SID_EXPERT_THUMP] == 70);
    chip_setup edit = kr.song_want;
    edit.profile = 3;
    edit.gate_reset = 0;
    edit.controls[2] = 50;
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_ENVELOPE | (1u << 2)) == 1);
    assert(song_char_base.profile == 3 && song_char_base.gate_reset == 0);
    assert(song_char_base.controls[0] == 10 && song_char_base.controls[1] == 20);
    assert(song_char_base.controls[2] == 50);
    assert(song_char_base.expert[SID_EXPERT_FILTER_6581_CURVE] == 70);
    assert(song_char_base.expert[SID_EXPERT_DAC_KINK] == 70);
    assert(song_char_base.expert[SID_EXPERT_THUMP] == SID_EXPERT_DEFAULT);
    assert(kr.song_want.controls[0] == 90 && kr.song_want.controls[1] == 80);
    edit = kr.song_want;
    edit.expert[SID_EXPERT_FILTER_6581_CURVE] = 91;
    assert(song_chip_edit(&kr, &edit,
                         1u << (KEYS_ROUTE_EDIT_EXPERT_FIRST + SID_EXPERT_FILTER_6581_CURVE)) == 1);
    assert(song_char_base.expert[SID_EXPERT_FILTER_6581_CURVE] == 91);
    assert(song_char_base.expert[SID_EXPERT_FILTER_6581_RANGE] == 70);
    assert(song_char_base.expert[SID_EXPERT_DAC_KINK] == 70);
    assert(kr.song_want.expert[SID_EXPERT_FILTER_6581_CURVE] == SID_EXPERT_DEFAULT);
    edit = kr.song_want;
    edit.controls[0] = 94;
    assert(song_chip_edit(&kr, &edit, 1u << 0) == 1);
    assert(song_char_base.controls[0] == 94 && song_char_base.controls[1] == 20);
    assert(song_char_base.expert[SID_EXPERT_FILTER_6581_CURVE] == SID_EXPERT_DEFAULT);
    assert(song_char_base.expert[SID_EXPERT_DAC_KINK] == 70);
    assert(kr.song_want.controls[0] == 90);
    song_chip_character(&kr, 1, 70);
    assert(kr.song_want.profile == 3 && kr.song_want.gate_reset == 0);
    assert(kr.song_want.controls[2] == 50);
    song.frozen = 1;
    edit.profile = 4;
    edit.controls[0] = 100;
    assert(song_chip_edit(&kr, &edit, 1u << 0) == 0);
    assert(toasts == 1 && song_char_base.profile == 3 && song_char_base.controls[0] == 94);
    assert(kr.song_want.profile == 3 && kr.song_want.controls[0] == 90);
    song.frozen = 0;
    song_chip_character(&kr, 0, -1);
    assert(kr.song_want.controls[0] == 94 && kr.song_want.controls[1] == 70);
    song_chip_character(&kr, 1, -1);
    assert(kr.song_want.controls[1] == 20 && kr.song_want.controls[2] == 50);
    edit = kr.song_want;
    edit.profile = 2;
    song.frozen = 1;
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_PROFILE) == 0);
    assert(toasts == 2 && kr.song_want.profile == 3);
    song.frozen = 0;
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_PROFILE) == 1);
    assert(kr.song_want.profile == 2);
    song_chip_character(&kr, 3, 60);
    assert(song_char_base.profile == 2 && song_char_base.controls[0] == 94);
    song_chip_character(&kr, 3, -1);
    base.controls[0] = 20;
    keys_route_song_wants(&kr, &base);
    song_chip_character(&kr, 0, 80);
    edit = kr.song_want;
    edit.controls[0] = 80; /* USER profile's explicit AGE matches the runtime lock. */
    edit.controls[1] = 50;
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_CONTROLS) == 1);
    assert(song_char_base.controls[0] == 80 && song_char_base.controls[1] == 50);
    song_chip_character(&kr, 0, -1);
    assert(kr.song_want.controls[0] == 80);
    keys_route_song_wants(&kr, &base);
    song_chip_character(&kr, 0, 80);
    edit = kr.song_want; /* CC AGE 80 is an edit even though the lock already asks for 80. */
    assert(song_chip_edit(&kr, &edit, 1u << 0) == 1);
    song_chip_character(&kr, 0, -1);
    assert(kr.song_want.controls[0] == 80);
    song_base_set(&base);
    edit = base;
    edit.profile = 4;
    edit.gate_reset = 0;
    edit.controls[0] = 70;
    edit.controls[1] = 110;
    keys_route_song_wants(&kr, &edit); /* A temporary MIDI ownership setup is not the song base. */
    assert(song_chip_edit(&kr, &edit, 1u << 0) == 1);
    assert(song_char_base.profile == base.profile && song_char_base.gate_reset == base.gate_reset);
    assert(song_char_base.controls[0] == 70 && song_char_base.controls[1] == base.controls[1]);
    assert(kr.song_want.profile == base.profile && kr.song_want.gate_reset == base.gate_reset);

    keys_route_song_wants(&kr, &base);
    song_chip_character(&kr, 0, 90);
    edit = kr.song_want;
    edit.expert[SID_EXPERT_FILTER_6581_CURVE] = 93;
    assert(song_chip_edit(&kr, &edit,
                         1u << (KEYS_ROUTE_EDIT_EXPERT_FIRST + SID_EXPERT_FILTER_6581_CURVE)) == 1);
    song_chip_character(&kr, 0, -1);
    assert(kr.song_want.expert[SID_EXPERT_FILTER_6581_CURVE] == 93);
    assert(kr.song_want.expert[SID_EXPERT_FILTER_6581_RANGE] == 70);
    assert(kr.song_want.expert[SID_EXPERT_DAC_KINK] == 70);

    song_chip_character(&kr, 0, 90);
    edit = kr.song_want;
    edit.profile = 4;
    edit.controls[0] = 90; /* USER AGE matches the lock but replaces the unlocked base. */
    memset(edit.expert, 88, sizeof edit.expert);
    assert(song_chip_edit(&kr, &edit,
                         KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_CONTROLS | KEYS_ROUTE_EDIT_EXPERT) == 1);
    assert(song_char_base.controls[0] == 90 && song_char_base.profile == 4);
    assert(memcmp(song_char_base.expert, edit.expert, sizeof edit.expert) == 0);
    song_chip_character(&kr, 0, -1);
    assert(memcmp(kr.song_want.expert, edit.expert, sizeof edit.expert) == 0);

    song_chip_character(&kr, 0, 110);
    kr.song_user = 2;
    chip_setup saved_base = song_char_base;
    chip_setup saved_runtime = kr.song_want;
    song.frozen = 1;
    edit.expert[SID_EXPERT_FILTER_6581_CURVE] = 12;
    assert(song_chip_edit(&kr, &edit,
                         1u << (KEYS_ROUTE_EDIT_EXPERT_FIRST + SID_EXPERT_FILTER_6581_CURVE)) == 0);
    edit.profile = 2;
    memset(edit.expert, SID_EXPERT_DEFAULT, sizeof edit.expert);
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_ALL) == 0);
    assert(toasts == 4 && kr.song_user == 2);
    assert(memcmp(&song_char_base, &saved_base, sizeof saved_base) == 0);
    assert(memcmp(&kr.song_want, &saved_runtime, sizeof saved_runtime) == 0);
    song.frozen = 0;
    song_chip_character(&kr, 0, -1);
    assert(memcmp(kr.song_want.expert, saved_base.expert, sizeof saved_base.expert) == 0);

    edit = kr.song_want;
    edit.profile = 2;
    memset(edit.expert, SID_EXPERT_DEFAULT, sizeof edit.expert);
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_PROFILE | KEYS_ROUTE_EDIT_EXPERT) == 1);
    assert(song_char_base.profile == 2 && kr.song_want.profile == 2 && kr.song_user == 0);
    assert(memcmp(song_char_base.expert, edit.expert, sizeof edit.expert) == 0);
    assert(memcmp(kr.song_want.expert, edit.expert, sizeof edit.expert) == 0);

    /* A .sid tune has the song's memory (M3): the edit goes straight to chip A's setup, the
     * song's base and the frozen song untouched. */
    song_away = 1;
    song.frozen = 1;
    saved_base = song_char_base;
    edit = kr.song_want;
    edit.profile = 3;
    assert(song_chip_edit(&kr, &edit, KEYS_ROUTE_EDIT_PROFILE) == 1);
    assert(kr.song_want.profile == 3 && toasts == 4);
    assert(memcmp(&song_char_base, &saved_base, sizeof saved_base) == 0);
    return 0;
}}
"#,
        &main[start..end]
    );
    let scratch = root.join("build/scratch");
    fs::create_dir_all(&scratch).unwrap();
    let file = scratch.join(format!("song-chip-edit-{}.c", std::process::id()));
    let exe = file.with_extension("test");
    fs::write(&file, source).unwrap();
    let build = Command::new("cc")
        .args(["-std=c11", "-Werror", "-I"])
        .arg(root.join("app"))
        .arg("-I")
        .arg(root.join("core"))
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    let run = build
        .status
        .success()
        .then(|| Command::new(&exe).output().unwrap());
    fs::remove_file(&file).unwrap();
    if exe.exists() {
        fs::remove_file(&exe).unwrap();
    }
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = run.unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn under_a_tune_keys_a_acts_as_auto() {
    let mut k = Keys::new();
    k.ticks(20);
    k.live.keys_mode = LIVE_KEYS_A as u8;
    k.song(true);
    unsafe { keys_route_set_tune(&mut *k.kr, 1) };
    assert_eq!(
        unsafe { keys_route_ready_for_song(&*k.kr) },
        0,
        "the keys still on A"
    );
    k.ticks(2);
    assert_eq!(k.live.keys_chip, 1, "the keys moved to chip B");
    assert_eq!(unsafe { keys_route_ready_for_song(&*k.kr) }, 1);
    unsafe { live_note_on(&mut *k.live, 60, 100) };
    k.tick();
    assert_eq!(
        unsafe { keys_route_voices_taken(&*k.kr) },
        0,
        "no voice of the tune's"
    );
    assert_eq!(gates(k.chip(1))[0], 1, "the key sounds on B");
    assert_eq!(gates(k.chip(0)), [0, 0, 0]);
    unsafe { live_note_off(&mut *k.live, 60) };
    unsafe { keys_route_set_tune(&mut *k.kr, 0) };
    k.song(false);
    k.ticks(30);
    assert_eq!(k.live.keys_chip, 0, "back on A after the tune");
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn a_2sid_tune_silences_the_keys_and_makes_both_chips_fresh() {
    let mut k = Keys::new();
    k.ticks(20);
    let mut want = chip_setup::default();
    unsafe { chip_setup_of(&*k.live, &mut want) };
    want.gate_reset = 0;
    let was = [k.chips.active[0], k.chips.active[1]];
    k.song(true);
    unsafe {
        keys_route_set_tune(&mut *k.kr, 1);
        keys_route_song_wants(&mut *k.kr, &want);
        k.kr.tune_b_want = want;
        keys_route_set_silent(&mut *k.kr, 1);
        assert_eq!(
            keys_route_ready_for_song(&*k.kr),
            1,
            "silent: ready at once"
        );
        keys_route_fresh(&mut *k.kr, 0);
        keys_route_fresh(&mut *k.kr, 1);
        live_note_on(&mut *k.live, 60, 100);
    }
    let mut n = 0;
    while k.chips.active[0] == was[0]
        || k.chips.active[1] == was[1]
        || unsafe {
            chip_ctl_has(&k.kr.ctl[0], &want) == 0 || chip_ctl_has(&k.kr.ctl[1], &want) == 0
        }
    {
        k.tick();
        n += 1;
        assert!(
            n < 200,
            "the chips were not made fresh: {:?}",
            k.chips.active
        );
    }
    assert_eq!(gates(k.chip(0)), [0, 0, 0], "the keys are silent on A");
    assert_eq!(gates(k.chip(1)), [0, 0, 0], "and on B");
    assert_eq!(k.live.keys_chip, 0, "they stay where they were");
    unsafe {
        live_note_off(&mut *k.live, 60);
        keys_route_set_silent(&mut *k.kr, 0);
        keys_route_set_tune(&mut *k.kr, 0);
    }
    k.song(false);
    k.ticks(30);
    unsafe { live_note_on(&mut *k.live, 64, 100) };
    k.tick();
    assert!(gates(k.chip(0)).contains(&1), "the keys sound on A again");
    assert_eq!(k.chips.lost_resets, 0);
}
