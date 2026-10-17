// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Which chip the keys play (`app/keys_route.c`) and each chip's setup (`app/chip_ctl.c`),
//! through the very ring the device runs: live on keys_route, chips A and B on one ring.

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

/// The device's instrument (platform/fm1/main.c): live on keys_route over two chips.
struct Keys {
    ring: Box<reg_ring>,
    chips: Box<reg_ring_chips>,
    outs: [Box<reg_ring_out>; 2],
    kr: Box<keys_route>,
    live: Box<live>,
    moves: Vec<u32>,
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

    /// A main-loop pass with a tick (platform/fm1/main.c): live's, then keys_route's.
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

/// The setup chip x has, as its ring sees it: the profile's filter rows and the ramp's
/// character.
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
    unsafe extern "C" fn refuse(ctx: *mut c_void, _: *const chip_setup) -> i32 {
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
    };
    assert_eq!(unsafe { keys_route_song_edit(&mut *k.kr, &edit) }, 0);
    assert_eq!(calls, 1);
    assert_eq!(k.kr.song_want.profile, before.profile);
    assert_eq!(k.kr.song_want.controls, before.controls);
    assert_eq!(k.kr.song_want.gate_reset, before.gate_reset);
    assert_eq!(k.kr.song_user, 2);
}

#[test]
fn chip_ctl_follows_a_setup_and_records_what_it_saw() {
    let mut k = Keys::new();
    let want = chip_setup {
        profile: SID_CHIP_6581_WORN as u8,
        controls: [40, 127, 64, 10],
        gate_reset: 0,
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

/// With no song the keys play chip A as in M1; chip B is made with the keys' setup in the
/// background (prepared, never reset at once), and holds it after every change: a profile, the
/// FX button, the controls, the envelope mode.
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

/// A song starts on chip A: the keys move to chip B (same setup), a key held across the start
/// is released on A and sounds again on B; new notes sound on B. The song stops with a key
/// held: the keys stay on B until it is released, then go back to A.
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
    // The song stops: two keys held, so the keys stay on B.
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

/// While a song plays on A the keys' setup changes act on chip B (the keys'), not on A (the
/// song's); when the song stops A is prepared to the keys' setup, and only then do the keys go
/// back to it.
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

/// KEYS A: during a song the keys stay on chip A; a key takes its voice from the song
/// (keys_route_voices_taken) while it sounds and gives it back at the release; the song keeps
/// the filter and the volume (the instrument's writes to $15-$18 do not reach the chip) and its
/// setup (the FX button does not reset the chip).
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
    // The song stops: chip A takes the keys' setup (the FX button's model) and filter.
    k.song(false);
    k.ticks(30);
    assert_eq!(k.chip(0).model, SID_MODEL_8580);
    let want = u32::from(k.live.cutoff_out);
    assert_eq!(k.chip(0).fc, want);
    assert_eq!(k.chips.lost_resets, 0);
}

/// The keys sound the same on either chip: the same key on chip A with no song and on chip B
/// during one, each chip fresh from the same setup, gives the same samples.
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
        // Each chip's voice 1 from its own last state: compare the envelopes and the
        // frequencies the chip got, then the sound of a fresh chip with those registers.
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

/// A pass at its heaviest with the keys moving in it (reg_ring.c, REG_RING_WORST_PASS): a
/// preset (every register), the FX button's request, every key down and up, then the move to
/// chip B (gate-offs and every register again), with no wait for room.
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

/// A voice's registers as chip x has them: frequency, pulse width, control, attack, decay,
/// sustain, release.
fn voice_regs(k: &mut Keys, x: u32, v: usize) -> (u32, u32, u8, u8, u8, u8, u8) {
    let s = k.chip(x);
    let (o, e) = (&s.osc[v], &s.env[v]);
    (
        o.freq, o.pw, o.control, e.attack, e.decay, e.sustain, e.release,
    )
}

/// The song's writes to chip A, as M2c's player would make them: every register of voices 1
/// to 3 with values of its own, the filter and the volume.
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

/// Review I-1: a song started before chip B is ready (here at once, chip B not made yet) finds
/// the keys still on chip A, which is then the song's already: keys_route_ready_for_song is 0,
/// and the instrument's setup, filter and untaken voices stay off chip A until the keys move.
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

/// Review I-2: the setup a song gives chip A (keys_route_song_setup) is what chip A is taken
/// back from after the song, with KEYS AUTO (chip A prepared to the keys' setup before the keys
/// return) and KEYS A (chip A prepared back under the keys); STATUS shows it meanwhile.
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

/// Review I-3: with KEYS A during a song, the voices the keys do not hold keep the song's
/// registers byte for byte: through a preset (every register), the PWM's every-tick pulse
/// widths, the envelope and pulse-width knobs and parameters, notes on the voice the keys
/// hold and its release, and a switch to KEYS A mid-song (a move and every register again).
/// Under mono sync the keys hold voice 3 with voice 1.
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
        live_set_preset(&mut *k.live, 1); // PWM BASS: its LFO writes pulse widths every tick
        live_note_on(&mut *k.live, 60, 100);
    }
    k.tick();
    let taken = unsafe { keys_route_voices_taken(&*k.kr) };
    assert_eq!(taken.count_ones(), 1);
    let held = taken.trailing_zeros() as usize;
    for t in 0..20 {
        unsafe {
            match t {
                3 => live_knob(&mut *k.live, LIVE_KNOB_PULSE_WIDTH, 9),
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
    // Mono sync (SYNC LEAD, preset 2): voice 1 and the sync source, voice 3.
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

/// The CHIP view's A tab during a song edits the song's setup (keys_route_song_wants):
/// chip A is prepared to it in the background, chip B (the keys') keeps the keys' setup.
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
    };
    assert!(k.kr.song_edit.is_none());
    assert!(k.kr.song_edit_ctx.is_null());
    assert_eq!(unsafe { keys_route_song_edit(&mut *k.kr, &song) }, 1);
    k.ticks(40);
    assert_eq!(k.chip(0).filter_curve, 3, "chip A has the song's WORN");
    assert_eq!(k.chips.ramp[0].now.thump, 64, "and its thump");
    assert_eq!(setup(&k, 1), b_before, "the keys' chip changed");
    assert_eq!(k.outs[0].unprepared_resets, 1, "live_init's only");
    assert_eq!(k.chips.lost_resets, 0);
}

/// What chip A has at the ring: its profile's filter row, its model and its ramp's controls.
fn chip_a_is(k: &Keys, want: &chip_setup) -> bool {
    let p = unsafe { &*sid_chip_profile_get(u32::from(want.profile)) };
    let c = &*k.chips;
    let s = &c.chip[usize::from(c.active[0])];
    let n = c.ramp[0].now;
    s.filter_curve == p.curve
        && s.model as u8 == p.model
        && [n.age, n.dac, n.thump, n.bleed] == want.controls
}

/// Review of Task 5, item 1: the want path is the one writer of song_want. An A-tab edit, then
/// the song's CHPA, then another edit: chip A follows each in turn, the latest wins, and what
/// keys_route says chip A has (ctl[0]) is what the ring's chip A has at the end.
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
    };
    let chpa = chip_setup {
        profile: SID_CHIP_6581_R2 as u8,
        controls: [0, 0, 64, 0],
        gate_reset: 1,
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

/// Review of Task 5, item 3: a song stops after A-tab edits (the profile and the controls),
/// with KEYS AUTO and KEYS A: chip A ends at the keys' profile and controls, with no reset made
/// at once and none lost.
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

/// M2e Task 3: a MIDI mode that gives chip A to the player counts as a song playing, with or
/// without one: the keys go to chip B, and come back when the mode ends or, if a song plays,
/// when it stops too.
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

/// M2e Task 3: the chip-level CCs on chip A's MIDI channels go to chip A's setup (the want
/// path), not to the keys' (live's), which stays on chip B.
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
    // CC 85 now includes USER 1..8 after the five built-in profiles.
    let worn = unsafe { midi_map_to_cc(midi_map_for_cc(85), SID_CHIP_6581_WORN) };
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
    // 25: the model, the 8580's profile.
    cc(&mut route, 0, 25, 127);
    assert_eq!(u32::from(k.kr.song_want.profile), SID_CHIP_8580_R5);
    cc(&mut route, 0, 25, 127);
    assert_eq!(
        u32::from(k.kr.song_want.profile),
        SID_CHIP_8580_R5,
        "already an 8580"
    );
    // Back to OMNI: chip A is the keys' again.
    s.mode = MIDI_MODE_OMNI as u8;
    unsafe { midi_route_set(&mut *route, &s) };
    k.ticks(60);
    assert_eq!(k.live.keys_chip, 0);
    assert_eq!(k.chips.lost_resets, 0);
}

#[test]
fn chip_edits_under_character_locks_preserve_the_unlocked_song_base() {
    use std::{fs, path::Path, process::Command};

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let main = fs::read_to_string(root.join("platform/fm1/main.c")).unwrap();
    let start = main.find("static chip_setup song_char_base;").unwrap();
    let end = main[start..].find("static void song_boot(void)").unwrap() + start;
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
static song_ctl* audio_song(void) {{ return &ctl; }}
static void ui_toast(const char* text, uint32_t ms) {{
    assert(strcmp(text, "SAVING") == 0 && ms == 2000u);
    toasts++;
}}
void keys_route_song_wants(keys_route* kr, const chip_setup* setup) {{ kr->song_want = *setup; }}
{}
int main(void) {{
    ctl.song = &song;
    keys_route kr = {{0}};
    chip_setup base = {{1, {{10, 20, 30, 40}}, 1}};
    keys_route_song_wants(&kr, &base);
    song_chip_character(&kr, 0, 90);
    song_chip_character(&kr, 1, 80);
    chip_setup edit = kr.song_want;
    edit.profile = 3;
    edit.gate_reset = 0;
    edit.controls[2] = 50;
    assert(song_chip_edit(&kr, &edit) == 1);
    assert(song_char_base.profile == 3 && song_char_base.gate_reset == 0);
    assert(song_char_base.controls[0] == 10 && song_char_base.controls[1] == 20);
    assert(song_char_base.controls[2] == 50);
    assert(kr.song_want.controls[0] == 90 && kr.song_want.controls[1] == 80);
    edit = kr.song_want;
    edit.controls[0] = 94;
    assert(song_chip_edit(&kr, &edit) == 1);
    assert(song_char_base.controls[0] == 94 && song_char_base.controls[1] == 20);
    assert(kr.song_want.controls[0] == 90);
    song_chip_character(&kr, 1, 70);
    assert(kr.song_want.profile == 3 && kr.song_want.gate_reset == 0);
    assert(kr.song_want.controls[2] == 50);
    song.frozen = 1;
    edit.profile = 4;
    edit.controls[0] = 100;
    assert(song_chip_edit(&kr, &edit) == 0);
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
    assert(song_chip_edit(&kr, &edit) == 0);
    assert(toasts == 2 && kr.song_want.profile == 3);
    song.frozen = 0;
    assert(song_chip_edit(&kr, &edit) == 1);
    assert(kr.song_want.profile == 2);
    song_chip_character(&kr, 3, 60);
    assert(song_char_base.profile == 2 && song_char_base.controls[0] == 94);
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
