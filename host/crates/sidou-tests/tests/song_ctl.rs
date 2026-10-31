// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::player::*;
use sidou_tests::song::*;
use sidou_tests::transport::*;

#[allow(clippy::too_many_arguments)]
fn ins(ad: u8, sr: u8, w: u8, p: u8, f: u8, vib: u8, vd: u8, gt: u8, fw: u8) -> [u8; 25] {
    let mut i = [0u8; 25];
    i[..9].copy_from_slice(&[ad, sr, w, p, f, vib, vd, gt, fw]);
    i
}

fn note(octave: u8, semi: u8) -> u8 {
    0x60 + octave * 12 + semi
}

fn shape_bytes(s: &gt2_shape) -> &[u8] {
    // SAFETY: gt2_shape is plain bytes and words with no padding between its fields.
    unsafe {
        std::slice::from_raw_parts((s as *const gt2_shape).cast::<u8>(), size_of::<gt2_shape>())
    }
}

fn analyse(song: &Song, sub: u32, o: &gt2_options, budget: Option<u32>) -> (i32, Box<gt2_shape>) {
    let mut a = Box::<gt2_analysis>::default();
    let mut shape = Box::<gt2_shape>::default();
    let r = unsafe {
        match budget {
            None => gt2_shape_analyse(&mut *a, song.ptr(), sub, o, &mut *shape),
            Some(b) => {
                let r = gt2_shape_begin(&mut *a, song.ptr(), sub, o);
                if r < 0 {
                    r
                } else {
                    let mut r;
                    let mut steps = 0u32;
                    loop {
                        r = gt2_shape_step(&mut *a, &mut *shape, b);
                        steps += 1;
                        assert!(steps < 10_000_000, "the analysis never ends");
                        if r != GT2_SHAPE_MORE as i32 {
                            break;
                        }
                    }
                    if r < 0 {
                        r
                    } else {
                        0
                    }
                }
            }
        }
    };
    (r, shape)
}

fn options_of(g: &gt2_ref::Options) -> gt2_options {
    let mut o = default_options();
    o.hr_adsr = g.hr_adsr as u16;
    o.multispeed = g.multiplier as u8;
    o.ntsc = g.ntsc as u8;
    o.optimize_pulse = g.optimize_pulse as u8;
    o.optimize_realtime = g.optimize_realtime as u8;
    let bits = [
        (gt2_ref::PLAYER_BUFFERED, GT2_PLAYER_BUFFERED),
        (gt2_ref::PLAYER_SOUNDEFFECTS, GT2_PLAYER_SOUNDEFFECTS),
        (gt2_ref::PLAYER_VOLUME, GT2_PLAYER_VOLUME),
        (gt2_ref::PLAYER_AUTHORINFO, GT2_PLAYER_AUTHORINFO),
        (gt2_ref::PLAYER_ZPGHOSTREGS, GT2_PLAYER_ZPGHOSTREGS),
        (gt2_ref::PLAYER_NOOPTIMIZATION, GT2_PLAYER_NOOPTIMIZATION),
        (gt2_ref::PLAYER_FULLBUFFERED, GT2_PLAYER_FULLBUFFERED),
    ];
    let mut f = 0u32;
    for (theirs, ours) in bits {
        if g.player_version & theirs != 0 {
            f |= ours;
        }
    }
    o.player_flags = f as u16;
    o
}

#[test]
fn the_sliced_analysis_makes_the_same_shape() {
    let mut cases: Vec<(String, Vec<u8>, gt2_options)> = gt2_ref::examples()
        .into_iter()
        .map(|(name, sng)| (name, sng, default_options()))
        .collect();
    for seed in 0..100 {
        let g = gt2_ref::gen::generate(seed);
        cases.push((format!("seed {seed}"), g.sng, options_of(&g.options)));
    }
    let mut shapes = 0;
    for (name, sng, o) in &cases {
        let song = Song::host_from(sng);
        for sub in 0..u32::from(sng[100]) {
            let (r, whole) = analyse(&song, sub, o, None);
            for budget in [1u32, 3, 64, SONG_SLICE_UNITS] {
                let (rs, sliced) = analyse(&song, sub, o, Some(budget));
                assert_eq!(rs, r, "{name} subtune {sub} budget {budget}");
                if r == 0 {
                    assert!(
                        shape_bytes(&whole) == shape_bytes(&sliced),
                        "{name} subtune {sub}: budget {budget} makes another shape"
                    );
                }
            }
            shapes += usize::from(r == 0);
        }
    }
    eprintln!(
        "{} songs, {shapes} shapes, equal in slices of 1, 3, 64 and {SONG_SLICE_UNITS}",
        cases.len()
    );
    assert!(shapes > 100);
}

#[test]
fn the_frame_clock_is_the_cia_period_exactly() {
    for ntsc in [0u32, 1] {
        for ms in 0u32..=4 {
            let p = unsafe { song_period_of(ms, ntsc) };
            let (base, clock) = if ntsc == 1 {
                (0x42c6u128, 1_022_727u128)
            } else {
                (0x4cc7, 985_248)
            };
            let cycles = if ms == 0 { base * 2 } else { base / ms as u128 } + 1;
            let lhs = (p.q as u128 * p.den as u128 + p.r as u128) * clock * 544;
            let rhs = cycles * 24_000_000 * p.den as u128;
            assert_eq!(lhs, rhs, "multispeed {ms} ntsc {ntsc}: {p:?}");
            assert!(p.r < p.den);
            assert!(p.q >= SONG_BLOCK, "a block holds at most one call");
        }
    }
    let pal = unsafe { song_period_of(1, 0) };
    assert_eq!(pal.q, 880, "19,656 cycles at 985,248 Hz is 880.16 samples");
}

fn short_song_patterns(channel_3_plays: bool) -> Vec<(Vec<[u8; 4]>, [u8; 4])> {
    let mut p0 = vec![
        row(note(3, 0), 1),
        REST,
        REST,
        REST,
        row(note(3, 4), 0),
        REST,
        REST,
        REST,
    ];
    p0.extend(vec![
        row(note(3, 7), 1),
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
        REST,
    ]);
    let p1 = vec![row(note(4, 0), 1), REST, REST, REST, REST, REST, REST, REST];
    let p2 = vec![row(note(2, 0), 1), REST, REST, REST, REST, REST, REST, REST];
    let p3 = if channel_3_plays {
        vec![row(note(1, 7), 1), REST, REST, REST, REST, REST, REST, REST]
    } else {
        vec![REST; 8]
    };
    vec![p0, p1, p2, p3]
        .into_iter()
        .map(|p| (p, [0xff, 0, 0, 0]))
        .collect()
}

fn simple_song() -> Vec<u8> {
    simple_spec().encode()
}

fn simple_spec() -> SngSpec {
    let mut s = SngSpec::new();
    s.orders = vec![vec![(vec![0, 1], 0), (vec![2], 0), (vec![3], 0)]];
    s.instr = vec![ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09)];
    s.tables = vec![vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]];
    s.patterns = short_song_patterns(true);
    s
}

fn song_of(sng: &[u8], o: Option<&gt2_options>) -> Song {
    let mut song = Song::host_from(sng);
    if let Some(o) = o {
        assert_eq!(unsafe { song_ext_set_options(song.ptr_mut(), o) }, 0);
    }
    song
}

fn transport_states(t: &mut Transport, calls: u32) -> Vec<Vec<u8>> {
    let mut states = Vec::new();
    let mut guard = 0u32;
    while t.calls() < calls {
        t.pass();
        let before = t.calls();
        t.block_silent();
        if t.calls() != before {
            states.push(player_state(&t.sc.player));
        }
        guard += 1;
        assert!(guard < 10_000_000);
    }
    states
}

fn reference_player_states(song: &Song, sub: u32, o: &gt2_options, calls: u32) -> Vec<Vec<u8>> {
    let mut p = Player::new(song, sub, o).expect("plays");
    (0..calls)
        .map(|_| {
            p.call_silent();
            player_state(p.raw())
        })
        .collect()
}

#[test]
fn the_transport_plays_what_the_player_plays_at_the_clocks_samples() {
    for (name, sng) in gt2_ref::examples().into_iter().take(4) {
        for ms in [1u8, 4] {
            let mut o = default_options();
            o.multispeed = ms;
            let mut song = song_of(&sng, Some(&o));
            let want = reference_player_states(&song, 0, &o, 600);
            let mut t = Transport::new(&mut song);
            assert!(t.play(0, 0));
            let mut got = Vec::new();
            let mut samples = Vec::new();
            let mut block = 0usize;
            let mut start = None;
            while got.len() < 600 {
                t.pass();
                let before = t.calls();
                let at = t.block_silent();
                if t.calls() != before {
                    if start.is_none() {
                        start = Some(block);
                    }
                    got.push(player_state(&t.sc.player));
                    samples.push((block - start.unwrap()) * BLOCK + at.unwrap());
                }
                block += 1;
            }
            assert!(got == want, "{name} x{ms}: the transport's player differs");
            let p = unsafe { song_period_of(ms as u32, 0) };
            for (k, &s) in samples.iter().enumerate() {
                let exact =
                    (k as u128 * (p.q as u128 * p.den as u128 + p.r as u128)) / p.den as u128;
                assert_eq!(s as u128, exact, "{name} x{ms}: call {k} at sample {s}");
            }
        }
    }
}

type Prim = Box<dyn Fn(*mut gt2_song, &mut gt2_edit) -> gt2_err>;

fn send_as_the_editor_does(t: &mut Transport, song: *mut gt2_song, p: &Prim) {
    for _ in 0..1000 {
        let mut e = gt2_edit::default();
        match p(song, &mut e) {
            GT2_OK => {
                if t.edit(&e) {
                    return;
                }
                unsafe { gt2_edit_cancel(song, &e) };
            }
            GT2_ERR_PENDING => {}
            err => panic!("the edit is refused: {err}"),
        }
        t.pass();
        t.block_silent();
    }
    panic!("an edit never went");
}

fn edit_case(name: &str, sng: &[u8], at: u32, expect_handover: bool, prims: &[Prim]) -> u32 {
    let o = default_options();
    let mut live = song_of(sng, None);
    let mut reference = song_of(sng, None);
    for p in prims {
        let mut e = gt2_edit::default();
        assert_eq!(p(reference.ptr_mut(), &mut e), GT2_OK, "{name}");
        unsafe { gt2_edit_commit(reference.ptr_mut(), &e) };
    }
    let song = live.ptr_mut();
    let mut t = Transport::new(&mut live);
    assert!(t.play(0, 0));
    transport_states(&mut t, at);
    let before = shape_bytes(t.shape()).to_vec();
    let sent = t.calls();
    for p in prims {
        send_as_the_editor_does(&mut t, song, p);
    }
    let handovers = t.sc.handovers;
    let mut states = Vec::new();
    let mut first = None;
    let total = at + 1500;
    while t.calls() < total {
        t.pass();
        let c = t.calls();
        t.block_silent();
        if t.calls() != c {
            let settled = t.sc.inflight_n == 0 || t.sc.cmd_tail == t.sc.stage;
            if first.is_none()
                && settled
                && t.sc.deriving == 0
                && (!expect_handover || t.sc.handovers > handovers)
            {
                first = Some(c);
            }
            states.push((c, player_state(&t.sc.player)));
        }
    }
    let first = first.unwrap_or_else(|| panic!("{name}: no hand-over"));
    assert_eq!(
        t.sc.handovers > handovers,
        expect_handover,
        "{name}: hand-over"
    );
    let want = reference_player_states(&reference, 0, &o, total);
    for (call, state) in states.iter().filter(|(c, _)| *c >= first) {
        assert!(
            *state == want[*call as usize],
            "{name}: call {call} differs from the edited song's player (edits at {sent}, \
             compared from {first})"
        );
    }
    assert!(
        before != shape_bytes(t.shape()),
        "{name}: the shape did not change"
    );
    let (r, fresh) = analyse(&reference, 0, &o, None);
    assert_eq!(r, 0);
    assert!(
        t.run_until(20_000, |t| t.sc.widened == 0
            && t.sc.bg_on == 0
            && t.sc.ff_on == 0),
        "{name}: the shape is never narrowed"
    );
    assert!(
        shape_bytes(t.shape()) == shape_bytes(&fresh),
        "{name}: the shape is not the fresh analysis's"
    );
    first - sent
}

fn set_row(pattern: u32, r: u32, cell: [u8; 4]) -> Prim {
    Box::new(move |s, e| unsafe { gt2_set_row(s, pattern, r, cell.as_ptr(), e) })
}

fn set_instr(i: u32, field: u32, v: u8) -> Prim {
    Box::new(move |s, e| unsafe { gt2_set_instr(s, i, field, v, e) })
}

fn set_table(t: u32, r: u32, side: u32, v: u8) -> Prim {
    Box::new(move |s, e| unsafe { gt2_set_table(s, t, r, side, v, e) })
}

fn insert_order(ch: u32, pos: u32, v: u8) -> Prim {
    Box::new(move |s, e| unsafe { gt2_order_insert(s, 0, ch, pos, v, e) })
}

fn insert_row(pattern: u32, r: u32) -> Prim {
    Box::new(move |s, e| unsafe { gt2_pattern_insert_row(s, pattern, r, e) })
}

fn edit_song(channel_3_plays: bool) -> Vec<u8> {
    let mut s = SngSpec::new();
    s.orders = vec![vec![(vec![0, 1], 0), (vec![2], 0), (vec![3], 0)]];
    s.instr = vec![
        ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09),
        ins(0x0a, 0x00, 3, 0, 0, 0, 0, 2, 0x09),
    ];
    s.tables = vec![
        vec![
            (0x41, 0x00),
            (0xff, 0x00),
            (0x11, 0x00),
            (0xff, 0x00),
            (0, 0),
            (0, 0),
        ],
        vec![(0, 0); 4],
        vec![(0, 0); 4],
        vec![(0, 0); 4],
    ];
    s.patterns = short_song_patterns(channel_3_plays);
    s.encode()
}

const EDIT_CALL: u32 = 40;

#[test]
fn a_note_past_the_range_widens_the_frequency_table() {
    edit_case(
        "note range",
        &edit_song(true),
        EDIT_CALL,
        false,
        &[set_row(1, 2, row(0xbc, 1))],
    );
}

#[test]
fn a_first_command_of_a_class_widens_the_handlers() {
    edit_case(
        "commands",
        &edit_song(true),
        EDIT_CALL,
        false,
        &[
            set_row(1, 3, [note(4, 2), 0, 5, 0x22]),
            set_row(1, 5, [0xbd, 0, 0x0f, 0x05]),
        ],
    );
}

#[test]
fn a_filter_command_writes_the_cutoff_from_then_on() {
    edit_case(
        "filter",
        &edit_song(true),
        EDIT_CALL,
        false,
        &[
            set_table(GT2_FTBL, 1, 0, 0x90),
            set_table(GT2_FTBL, 1, 1, 0xf1),
            set_table(GT2_FTBL, 2, 0, 0x00),
            set_table(GT2_FTBL, 2, 1, 0x40),
            set_row(1, 1, [0xbd, 0, 0x0a, 0x01]),
        ],
    );
}

#[test]
fn a_new_instrument_reaches_its_tables() {
    edit_case(
        "instrument",
        &edit_song(true),
        EDIT_CALL,
        false,
        &[
            set_table(GT2_WTBL, 3, 1, 0x8c),
            set_table(GT2_STBL, 1, 0, 0x80),
            set_table(GT2_STBL, 1, 1, 0x02),
            set_row(1, 0, [note(4, 0), 2, 1, 0x01]),
        ],
    );
}

#[test]
fn an_orderlist_rebuild_plays_its_new_pattern() {
    edit_case(
        "orderlist",
        &edit_song(true),
        EDIT_CALL,
        false,
        &[
            insert_order(0, 2, 2),
            insert_row(1, 4),
            set_row(1, 4, [note(5, 1), 1, 0, 0]),
        ],
    );
}

#[test]
fn a_wave_delay_hands_the_player_over() {
    let took = edit_case(
        "wave delay",
        &edit_song(true),
        EDIT_CALL,
        true,
        &[
            set_table(GT2_WTBL, 3, 0, 0x03),
            set_row(1, 0, row(note(4, 0), 2)),
        ],
    );
    eprintln!("the wave delay's hand-over: {took} calls after the edit");
}

#[test]
fn a_pulse_low_nybble_hands_the_player_over() {
    edit_case(
        "pulse",
        &edit_song(true),
        EDIT_CALL,
        true,
        &[
            set_table(GT2_PTBL, 1, 0, 0x88),
            set_table(GT2_PTBL, 1, 1, 0x08),
            set_instr(2, GT2_INS_PULSE, 1),
            set_row(1, 0, row(note(4, 0), 2)),
        ],
    );
}

#[test]
fn a_channels_first_note_hands_the_player_over() {
    let took = edit_case(
        "channels",
        &edit_song(false),
        EDIT_CALL,
        true,
        &[set_row(3, 0, row(note(2, 7), 1))],
    );
    eprintln!("the third channel's hand-over: {took} calls after the edit");
    assert!(took < 50, "the hand-over took {took} calls");
}

fn play_simple() -> (Song, Transport) {
    let mut song = song_of(&simple_song(), None);
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, 0));
    assert!(t.run_until(100, |t| t.calls() > 0));
    t.pass();
    (song, t)
}

fn env_state(t: &Transport, v: usize) -> u32 {
    t.chip.env[v].state
}

#[test]
fn stop_releases_continue_resumes_mute_silences() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 30);
    let gated: Vec<bool> = (0..3).map(|v| t.chip.env[v].gate != 0).collect();
    assert!(gated.iter().any(|&g| g), "a voice holds a note");
    assert!(t.stop());
    t.pass();
    t.block_silent();
    for (v, &was) in gated.iter().enumerate() {
        assert_eq!(t.chip.env[v].gate, 0, "voice {v}'s gate after stop");
        if was {
            assert_eq!(env_state(&t, v), SID_ENV_RELEASE, "voice {v} releases");
        }
    }
    let calls = t.calls();
    for _ in 0..50 {
        t.pass();
        t.block_silent();
    }
    assert_eq!(t.calls(), calls, "no call while stopped");
    assert!(t.resume());
    t.pass();
    let at = t.block_silent();
    assert_eq!(at, Some(0), "continue: the call at the block's start");
    assert!(t.mute(1));
    t.pass();
    t.block_silent();
    assert_eq!(t.chip.env[0].gate, 0, "the muted voice's gate");
    let freq = t.chip.osc[0].freq;
    let shadow = t.sc.shadow;
    let until = t.calls() + 300;
    transport_states(&mut t, until);
    assert_eq!(
        t.chip.osc[0].freq, freq,
        "the muted voice's frequency stays"
    );
    assert_eq!(t.chip.env[0].gate, 0);
    assert!(
        t.sc.shadow[..2] != shadow[..2],
        "the player wrote it all the same"
    );
    assert!(t.mute(0));
    t.pass();
    t.block_silent();
    let f = u32::from(t.sc.shadow[0]) | u32::from(t.sc.shadow[1]) << 8;
    assert_ne!(t.chip.osc[0].freq, freq);
    assert_eq!(t.chip.env[0].gate, t.sc.shadow[4] & 1);
    let _ = f;
}

#[test]
fn a_flood_of_edits_waits_and_is_never_lost() {
    let (mut song, mut t) = play_simple();
    let s = song.ptr_mut();
    let mut refused = 0;
    let mut sent = 0;
    let target = 200u32;
    while sent < target {
        let r = sent % 8;
        let mut e = gt2_edit::default();
        let cell = [note(3, (sent % 12) as u8), 1, 0, 0];
        match unsafe { gt2_set_row(s, 2, r, cell.as_ptr(), &mut e) } {
            GT2_OK => {}
            GT2_ERR_PENDING => {
                t.pass();
                t.block_silent();
                continue;
            }
            err => panic!("{err}"),
        }
        if t.edit(&e) {
            sent += 1;
        } else {
            unsafe { gt2_edit_cancel(s, &e) };
            refused += 1;
            if refused % 4 == 0 {
                t.pass();
                t.block_silent();
            }
        }
    }
    assert!(t.run_until(100, |t| t.sc.inflight_n == 0));
    assert!(refused > 0, "the flood never filled the queue");
    assert_eq!(t.sc.busy, refused, "every refusal is counted");
    assert!(t.sc.edits_block_max <= SONG_EDITS_PER_BLOCK && t.sc.edits_block_max > 1);
    let last = song.pattern(2).unwrap();
    for r in 0..8u32 {
        let k = (0..target).rev().find(|k| k % 8 == r).unwrap();
        assert_eq!(
            last[r as usize][0],
            note(3, (k % 12) as u8),
            "row {r}: the last edit"
        );
    }
    eprintln!(
        "200 edits, {refused} refusals, at most {} a block",
        t.sc.edits_block_max
    );
}

struct SplitDeleteSwap {
    edit: gt2_edit,
    old_bytes: Vec<u8>,
}

fn split_delete_units(edit: &gt2_edit, span: gt2_span) -> std::ops::Range<u32> {
    let units = if edit.slot as u32 >= GT2_SLOT_ORDER {
        span_len(span).div_ceil(GT2_UNIT)
    } else {
        span_len(span)
    };
    span_off4(span)..span_off4(span) + units
}

fn split_delete_old_bytes(song: &Song, edit: &gt2_edit) -> Vec<u8> {
    let units = split_delete_units(edit, edit.old_span);
    let start = song.raw().arena_offset as usize + units.start as usize * GT2_UNIT as usize;
    song.bytes()[start..start + units.len() * GT2_UNIT as usize].to_vec()
}

fn split_delete_check_model(song: &Song, model: &SngSpec) {
    assert_eq!(song.raw().subtunes as usize, model.orders.len());
    assert_eq!(song.raw().channels, 3);
    for (subtune, channels) in model.orders.iter().enumerate() {
        for (channel, (entries, restart)) in channels.iter().enumerate() {
            let mut prefix = 0;
            for &entry in entries {
                match entry {
                    0..=0xcf => {
                        assert!(
                            (entry as usize) < model.patterns.len(),
                            "a dangling pattern"
                        );
                        prefix = 0;
                    }
                    0xd0..=0xdf if prefix <= 1 => prefix = 2,
                    0xe0..=0xfe if prefix == 0 => prefix = 1,
                    _ => panic!("an invalid intermediate orderlist: {entries:?}"),
                }
            }
            assert_eq!(prefix, 0, "an unfinished orderlist prefix");
            assert!(!entries.is_empty() && (*restart as usize) < entries.len());
            assert_eq!(
                song.order(subtune as u32, channel as u32).unwrap(),
                (entries.clone(), *restart),
                "subtune {subtune} channel {channel}"
            );
        }
    }
    for pattern in 0..GT2_MAX_PATTERNS as usize {
        let expected = model.patterns.get(pattern).map(|(body, end)| {
            let mut rows = body.clone();
            rows.push(*end);
            rows
        });
        assert_eq!(song.pattern(pattern as u32), expected, "pattern {pattern}");
    }
}

fn split_delete_tick(
    t: &mut Transport,
    song: &mut Song,
    model: &mut SngSpec,
    swaps: &[SplitDeleteSwap],
    applied: &mut usize,
    released: &mut usize,
    max_per_block: &mut u32,
) {
    split_delete_check_model(song, model);
    let bitmap = song.bitmap();
    for swap in &swaps[*released..] {
        assert!(split_delete_units(&swap.edit, swap.edit.old_span).all(|u| bitmap[u as usize]));
        assert_eq!(split_delete_old_bytes(song, &swap.edit), swap.old_bytes);
    }
    t.pass();
    let bitmap = song.bitmap();
    for swap in &swaps[*released..*applied] {
        assert!(
            split_delete_units(&swap.edit, swap.edit.old_span).all(|u| !bitmap[u as usize]),
            "acknowledged old copies must be released by the poll"
        );
    }
    *released = *applied;
    split_delete_check_model(song, model);
    let before = t.sc.edit_swaps;
    t.block_silent();
    let count = t.sc.edit_swaps - before;
    assert!(count <= SONG_EDITS_PER_BLOCK, "{count} swaps in one block");
    assert_eq!(t.sc.edit_cells, 0, "the split edit must use swaps only");
    *max_per_block = (*max_per_block).max(count);
    let now_applied = *applied + count as usize;
    assert!(
        now_applied <= swaps.len(),
        "an unsubmitted swap was counted"
    );
    for index in *applied..now_applied {
        if index < 96 {
            let entries = &mut model.orders[index / 3][index % 3].0;
            for entry in entries {
                if *entry == 3 {
                    *entry = 0;
                }
            }
        } else {
            assert_eq!(index, 96);
            model.patterns.pop();
        }
    }
    *applied = now_applied;
    split_delete_check_model(song, model);
    let bitmap = song.bitmap();
    for swap in &swaps[*released..] {
        assert!(
            split_delete_units(&swap.edit, swap.edit.old_span).all(|u| bitmap[u as usize]),
            "the interrupt must retain old copies until acknowledgement"
        );
        assert_eq!(split_delete_old_bytes(song, &swap.edit), swap.old_bytes);
    }
    if *applied == 97 && *released < 97 {
        let mut pattern = 0;
        let mut probe = gt2_edit::default();
        assert_eq!(
            unsafe { gt2_pattern_add(song.ptr_mut(), 1, &mut pattern, &mut probe) },
            GT2_OK
        );
        assert_ne!(
            pattern, 3,
            "a removed pattern number cannot be reused before release"
        );
        unsafe { gt2_edit_cancel(song.ptr_mut(), &probe) };
    }
}

#[test]
fn deleting_a_pattern_across_32_subtunes_uses_97_bounded_acknowledged_swaps() {
    for playing in [false, true] {
        split_delete_case(playing);
    }
}

fn split_delete_case(playing: bool) {
    let mut fixture = SngSpec::new();
    fixture.orders = (0..32)
        .map(|subtune| {
            (0..3)
                .map(|channel| {
                    (
                        vec![0, 0xe0 + subtune % 16, 0xd0 + channel, 3, 3],
                        if subtune % 2 == 0 { 0 } else { 4 },
                    )
                })
                .collect()
        })
        .collect();
    fixture.instr = vec![ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09)];
    fixture.tables = vec![vec![(0x41, 0x00), (0xff, 0x00)], vec![], vec![], vec![]];
    let mut body = vec![REST; GT2_MAX_ROWS as usize];
    body[0] = row(note(3, 0), 1);
    fixture.patterns = vec![(body, [0xff, 0, 0, 0]); 4];
    let mut model = fixture.clone();
    let mut song = Song::device();
    song.read(&fixture.encode(), 4).unwrap();
    let mut t = Transport::new(&mut song);
    if playing {
        assert!(t.play(0, 0));
        transport_states(&mut t, 3);
        t.pass();
    }
    assert_eq!((t.sc.edit_cells, t.sc.edit_swaps), (0, 0));
    let mut queued = 0;
    for _ in 0..=SONG_CMD_RING {
        if !t.mute(0) {
            break;
        }
        queued += 1;
    }
    assert!(
        queued > 0 && !t.mute(0),
        "exercise a genuinely full command ring"
    );
    let mut swaps = Vec::<SplitDeleteSwap>::new();
    let (mut applied, mut released) = (0, 0);
    let (mut refused, mut removal_waits, mut reused_after_ack, mut max_per_block) = (0, 0, 0, 0);
    let mut pending = None;
    for _ in 0..20_000 {
        if released == 97 {
            break;
        }
        if swaps.len() < 97 {
            if pending.is_none() {
                let index = swaps.len();
                let mut edit = gt2_edit::default();
                let before = song.bytes().to_vec();
                let result = if index < 96 {
                    let (entries, restart) = &fixture.orders[index / 3][index % 3];
                    let replacement: Vec<u8> = entries
                        .iter()
                        .map(|&p| if p == 3 { 0 } else { p })
                        .collect();
                    unsafe {
                        gt2_order_replace(
                            song.ptr_mut(),
                            (index / 3) as u32,
                            (index % 3) as u32,
                            replacement.as_ptr(),
                            replacement.len() as u32,
                            *restart,
                            &mut edit,
                        )
                    }
                } else {
                    unsafe { gt2_pattern_remove(song.ptr_mut(), 3, &mut edit) }
                };
                match result {
                    GT2_OK => {
                        assert_eq!(edit.kind as u32, GT2_EDIT_SWAP);
                        if edit.span != 0 {
                            let new = split_delete_units(&edit, edit.span);
                            for (old_index, old) in swaps.iter().enumerate() {
                                let old = split_delete_units(&old.edit, old.edit.old_span);
                                let overlaps = new.start < old.end && old.start < new.end;
                                if old_index < released {
                                    reused_after_ack += usize::from(overlaps);
                                } else {
                                    assert!(
                                        !overlaps,
                                        "a new copy reused an unacknowledged old span"
                                    );
                                }
                            }
                        }
                        pending = Some(edit);
                    }
                    GT2_ERR_PENDING => {
                        assert_eq!(
                            index, 96,
                            "only the removal must wait for order acknowledgements"
                        );
                        removal_waits += 1;
                        assert_eq!(song.bytes(), before);
                    }
                    error => panic!("split edit {index}: {error}"),
                }
                split_delete_check_model(&song, &model);
            }
            if let Some(edit) = pending {
                if t.edit(&edit) {
                    swaps.push(SplitDeleteSwap {
                        edit,
                        old_bytes: split_delete_old_bytes(&song, &edit),
                    });
                    pending = None;
                    continue;
                }
                refused += 1;
            }
        }
        split_delete_tick(
            &mut t,
            &mut song,
            &mut model,
            &swaps,
            &mut applied,
            &mut released,
            &mut max_per_block,
        );
    }
    assert_eq!((swaps.len(), applied, released), (97, 97, 97));
    assert!(pending.is_none());
    assert!(refused > 0, "the stream must fill the queue and retry");
    assert!(
        removal_waits > 0,
        "removal must wait for the orderlist acknowledgements"
    );
    assert!(
        reused_after_ack > 0,
        "released old space must actually be reused"
    );
    assert!(max_per_block > 0 && max_per_block <= SONG_EDITS_PER_BLOCK);
    if !playing {
        assert_eq!(
            max_per_block, SONG_EDITS_PER_BLOCK,
            "exercise a full block of swaps"
        );
    }
    assert_eq!(t.sc.edits_block_max, max_per_block);
    assert_eq!((t.sc.edit_cells, t.sc.edit_swaps), (0, 97));
    assert_eq!((song.raw().pending, song.raw().pending_old_units), (0, 0));
    let mut number = 0;
    let mut probe = gt2_edit::default();
    assert_eq!(
        unsafe { gt2_pattern_add(song.ptr_mut(), 1, &mut number, &mut probe) },
        GT2_OK
    );
    assert_eq!(number, 3, "the acknowledged pattern number is reusable");
    unsafe { gt2_edit_cancel(song.ptr_mut(), &probe) };
    split_delete_check_model(&song, &model);
    let reference = song_of(&model.encode(), None);
    if !playing {
        assert!(t.play(0, 0));
        transport_states(&mut t, 3);
    }
    let first = t.calls();
    let got = transport_states(&mut t, first + 24);
    let expected = reference_player_states(&reference, 0, &default_options(), first + 24);
    assert_eq!(
        got,
        expected[first as usize..],
        "playback after the split edit"
    );
    eprintln!(
        "playing={playing}: 97 swaps, {refused} queue retries, {removal_waits} removal waits, \
         {reused_after_ack} acknowledged reuses, at most {max_per_block} swaps/block"
    );
}

#[test]
fn a_swapped_patterns_old_space_is_freed_only_after_it_is_applied() {
    let (mut song, mut t) = play_simple();
    let s = song.ptr_mut();
    let old = song.raw().pattern[2];
    let mut e = gt2_edit::default();
    assert_eq!(unsafe { gt2_pattern_insert_row(s, 2, 1, &mut e) }, GT2_OK);
    let used = |song: &Song, sp: u32| -> bool {
        let bm = song.bitmap();
        (span_off4(sp)..span_off4(sp) + 1).all(|u| bm[u as usize])
    };
    assert!(t.edit(&e));
    t.pass();
    assert!(used(&song, old), "pending: the old copy is allocated");
    assert_eq!(song.raw().pattern[2], old, "not applied before a block");
    t.block_silent();
    assert_ne!(song.raw().pattern[2], old, "applied at the block");
    assert!(used(&song, old), "applied, not yet known applied");
    t.pass();
    assert!(
        !used(&song, old),
        "released after the main loop saw it applied"
    );
}

#[test]
fn a_jam_plays_at_the_voices_next_tick_0() {
    let (_song, mut t) = play_simple();
    t.events();
    assert!(t.jam(1, 30, 1));
    let mut found = false;
    for _ in 0..60 {
        t.pass();
        t.block_silent();
        for e in t.events() {
            if (e.flags >> 5) & 3 == 1
                && u32::from(e.flags & 0x1f) & GT2_EV_NOTE != 0
                && e.note == 30
            {
                found = true;
            }
        }
    }
    assert!(found, "the jammed note started");
}

#[test]
fn a_seek_hands_over_a_fast_forwarded_player() {
    let (song, mut t) = play_simple();
    assert!(t.seek(500));
    assert!(t.run_until(5000, |t| t.sc.handovers > 0));
    let o = default_options();
    let want = reference_player_states(&song, 0, &o, 700);
    let calls = t.calls();
    let got = transport_states(&mut t, calls + 100);
    for (k, s) in got.iter().enumerate() {
        assert!(
            *s == want[calls as usize + k],
            "call {} after the seek",
            calls as usize + k
        );
    }
}

#[test]
fn the_events_follow_the_player_and_overflow_drops_the_oldest() {
    let (song, mut t) = play_simple();
    t.events();
    let start = t.calls();
    let mut p = Player::new(&song, 0, &default_options()).unwrap();
    for _ in 0..start {
        p.call_silent();
    }
    let mut want = Vec::new();
    for _ in 0..200 {
        let k = t.calls();
        p.call_silent();
        for ch in 0..3u8 {
            let ev = p.raw().ev[ch as usize];
            if ev.flags != 0 {
                want.push((k as u16, ev.flags | ch << 5, ev.pattern, ev.row, ev.note));
            }
        }
        t.pass();
        while t.calls() == k {
            t.block_silent();
        }
    }
    let got: Vec<_> = t
        .events()
        .iter()
        .map(|e| (e.frame, e.flags, e.pattern, e.row, e.note))
        .collect();
    assert_eq!(got, want);
    let lost = t.sc.ev_lost;
    let until = t.calls() + 2000;
    transport_states(&mut t, until);
    let kept = t.events();
    assert_eq!(kept.len(), SONG_EVENT_RING as usize);
    assert!(t.sc.ev_lost > lost, "the dropped ones are counted");
    let frames: Vec<u32> = kept.iter().map(|e| u32::from(e.frame)).collect();
    assert!(frames.windows(2).all(|w| w[0] <= w[1]), "oldest first");
    assert!(
        frames[frames.len() - 1] + 30 >= (t.calls() - 1) & 0xffff,
        "the newest kept"
    );
}

#[test]
fn again_says_whether_a_repeated_pattern_plays_once_more() {
    let mut spec = simple_spec();
    spec.orders[0][0].0 = vec![GT2_REPEAT as u8 + 2, 0, 1];
    let mut song = song_of(&spec.encode(), None);
    let mut t = Transport::new(&mut song);
    assert_eq!(unsafe { song_ctl_again(&*t.sc) }, 0, "nothing plays");
    assert!(t.play(0, 0));
    let mut last: Option<(u8, bool)> = None;
    let mut seen = Vec::new();
    let mut guard = 0;
    while seen.len() < 6 {
        t.pass();
        t.block_silent();
        let again = unsafe { song_ctl_again(&*t.sc) } & 1 != 0;
        let events = t.events();
        if let Some((_, a)) = last {
            if events.is_empty() {
                assert_eq!(again, a, "the whole last row says the same, to its end");
            }
        }
        for e in events {
            if u32::from(e.flags & 0x1f) & GT2_EV_ROW == 0 || (e.flags >> 5) & 3 != 0 {
                continue;
            }
            let rows = unsafe { gt2_pattern_rows(song.ptr(), u32::from(e.pattern)) } - 1;
            if e.row == 0 {
                if let Some((p, a)) = last.take() {
                    assert_eq!(a, e.pattern == p, "pattern {p} then {}", e.pattern);
                    seen.push((p, a));
                }
            }
            if u32::from(e.row) + 1 == rows {
                last = Some((e.pattern, again));
            }
        }
        guard += 1;
        assert!(guard < 100_000);
    }
    assert_eq!(
        seen,
        [
            (0, true),
            (0, true),
            (0, false),
            (1, false),
            (0, true),
            (0, true)
        ],
        "D2: three passes of 0, then 1, round"
    );
}

#[test]
fn multispeed_past_4_is_refused() {
    let mut o = default_options();
    o.multispeed = 8;
    let mut song = song_of(&simple_song(), Some(&o));
    let mut t = Transport::new(&mut song);
    assert!(t.play(0, 0));
    assert_eq!(t.sc.error, SONG_ERR_MULTISPEED as i16);
    assert_eq!(t.state(), SONG_IDLE);
}

#[test]
fn a_swapped_chip_gets_the_players_registers() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 20);
    let mut fresh = Box::<sid>::default();
    unsafe {
        sid_init(&mut *fresh, SID_MODEL_6581, SID_CLOCK_PAL, SAMPLE_RATE);
        song_ctl_chip_swapped((&mut *t.sc as *mut song_ctl).cast(), 0, &mut *fresh);
    }
    for v in 0..3 {
        assert_eq!(fresh.env[v].gate, t.chip.env[v].gate, "voice {v}'s gate");
        assert_eq!(
            fresh.env[v].attack, t.chip.env[v].attack,
            "voice {v}'s attack"
        );
        assert_eq!(
            fresh.env[v].sustain, t.chip.env[v].sustain,
            "voice {v}'s sustain"
        );
        assert_eq!(
            fresh.osc[v].freq, t.chip.osc[v].freq,
            "voice {v}'s frequency"
        );
        assert_eq!(fresh.osc[v].pw, t.chip.osc[v].pw, "voice {v}'s pulse width");
    }
    assert_eq!(fresh.mode_vol, t.chip.mode_vol);
    assert_eq!(fresh.fc, t.chip.fc);
}

#[test]
fn a_voice_the_keys_take_is_left_to_them_and_given_back() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 30);
    assert!(t.take(1));
    t.pass();
    t.block_silent();
    unsafe {
        sid_write(&mut *t.chip, 0, 0x34);
        sid_write(&mut *t.chip, 1, 0x12);
        sid_write(&mut *t.chip, 4, 0x41);
    }
    let until = t.calls() + 200;
    transport_states(&mut t, until);
    assert_eq!(t.chip.osc[0].freq, 0x1234, "the keys' frequency stands");
    assert_eq!(t.chip.env[0].gate, 1, "the keys' gate stands");
    assert!(t.take(0));
    t.pass();
    t.block_silent();
    let f = u32::from(t.sc.shadow[0]) | u32::from(t.sc.shadow[1]) << 8;
    assert_eq!(t.chip.osc[0].freq, f, "the song's frequency again");
    assert_eq!(t.chip.env[0].gate, t.sc.shadow[4] & 1);
}

#[test]
fn a_take_holds_from_the_next_block_with_no_command_and_no_pass() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 30);
    let staged = t.sc.stage;
    assert!(t.take(1));
    assert_eq!(t.sc.stage, staged, "no command: nothing waits in the ring");
    unsafe {
        sid_write(&mut *t.chip, 0, 0x34);
        sid_write(&mut *t.chip, 1, 0x12);
        sid_write(&mut *t.chip, 4, 0x41);
    }
    let until = t.calls() + 20;
    while t.calls() < until {
        t.block_silent();
        assert_eq!(t.chip.osc[0].freq, 0x1234, "the keys' frequency stands");
        assert_eq!(t.chip.env[0].gate, 1, "the keys' gate stands");
    }
    assert!(t.take(0));
    t.block_silent();
    let f = u32::from(t.sc.shadow[0]) | u32::from(t.sc.shadow[1]) << 8;
    assert_eq!(t.chip.osc[0].freq, f, "the song's frequency again");
    assert_eq!(t.chip.env[0].gate, t.sc.shadow[4] & 1);
}

#[test]
fn play_leaves_the_voices_the_keys_hold_sounding() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 30);
    assert!(t.take(1));
    t.block_silent();
    unsafe {
        sid_write(&mut *t.chip, 0, 0x34);
        sid_write(&mut *t.chip, 1, 0x12);
        sid_write(&mut *t.chip, 4, 0x41);
    }
    let block = t.sc.play_block;
    assert!(t.play(0, 0));
    assert!(t.run_until(1000, |t| t.sc.play_block != block && t.calls() > 2));
    assert_eq!(t.chip.osc[0].freq, 0x1234, "the keys' frequency stands");
    assert_eq!(t.chip.env[0].gate, 1, "the keys' gate stands");
    for v in 1..3 {
        assert_eq!(
            t.chip.osc[v].freq,
            u32::from(t.sc.shadow[v * 7]) | u32::from(t.sc.shadow[v * 7 + 1]) << 8,
            "voice {v} is the song's"
        );
    }
    assert!(t.take(0));
    t.block_silent();
    let f = u32::from(t.sc.shadow[0]) | u32::from(t.sc.shadow[1]) << 8;
    assert_eq!(t.chip.osc[0].freq, f, "the song's frequency again");
}

#[test]
fn a_jam_release_lets_the_voice_go_at_the_next_call() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 20);
    let gated = (0..3)
        .find(|&v| t.chip.env[v].gate != 0)
        .expect("a voice holds a note");
    assert!(t.jam(gated as u32, SONG_JAM_RELEASE, 0));
    t.pass();
    let calls = t.calls();
    assert!(t.run_until(20, |t| t.calls() > calls + 1));
    assert_eq!(t.chip.env[gated].gate, 0, "voice {gated} let go");
    assert_eq!(t.sc.shadow[gated * 7 + 4] & 1, 0);
}

fn calls_and_row_events(t: &mut Transport, blocks: usize) -> (u32, usize) {
    let calls = t.calls();
    let mut rows = 0;
    for _ in 0..blocks {
        t.pass();
        t.block_silent();
        rows += t
            .events()
            .iter()
            .filter(|e| u32::from(e.flags & 0x1f) & GT2_EV_ROW != 0)
            .count();
    }
    (t.calls() - calls, rows)
}

#[test]
fn an_audition_sounds_jams_with_the_song_stopped() {
    let mut song = song_of(&simple_song(), None);
    let mut t = Transport::new(&mut song);
    assert!(!t.jam_ready());
    assert!(t.audition());
    assert_eq!(
        unsafe { song_ctl_shown_state(&*t.sc) },
        SONG_IDLE,
        "no start shown"
    );
    assert!(t.run_until(200, |t| t.jam_ready()));
    assert_eq!(t.state(), SONG_STOPPED);
    assert!(t.jam(2, 30, 1));
    t.pass();
    let calls = t.calls();
    assert!(t.run_until(20, |t| t.calls() > calls + 1));
    assert_ne!(t.chip.env[2].gate, 0, "the jam sounds on voice 3");
    let f = u32::from(t.sc.shadow[14]) | u32::from(t.sc.shadow[15]) << 8;
    assert_ne!(f, 0, "at the note's frequency");
    let (calls, rows) = calls_and_row_events(&mut t, 100);
    assert!(calls >= 14, "the calls run, 50 a second: {calls}");
    assert_eq!(rows, 0, "no row read with the song stopped");
    for v in 0..2 {
        assert_eq!(t.chip.env[v].gate, 0, "voice {v} stays silent");
    }
    assert!(t.jam(2, SONG_JAM_RELEASE, 1));
    calls_and_row_events(&mut t, 5);
    assert_eq!(t.chip.env[2].gate, 0, "released");
    assert!(t.audition_end());
    calls_and_row_events(&mut t, 2);
    let (calls, _) = calls_and_row_events(&mut t, 50);
    assert_eq!(calls, 0, "no call once it ends");
    assert!(!t.jam_ready());
}

#[test]
fn an_audition_after_a_stop_keeps_where_continue_goes_on() {
    let (_song, mut t) = play_simple();
    transport_states(&mut t, 40);
    assert!(t.stop());
    for _ in 0..4 {
        t.pass();
        t.block_silent();
    }
    assert_eq!(t.sc.chip_held, 0, "the stop handed chip A back");
    let stopped = t.sc.player.chn.map(|c| (c.songptr, c.pattptr));
    assert!(t.audition());
    assert!(t.run_until(20, |t| t.jam_ready()));
    assert!(t.jam(0, 40, 1));
    calls_and_row_events(&mut t, 60);
    assert!(t.audition_end());
    calls_and_row_events(&mut t, 4);
    assert_eq!(
        t.sc.player.chn.map(|c| (c.songptr, c.pattptr)),
        stopped,
        "the audition read no row"
    );
    assert!(t.resume());
    assert!(t.run_until(20, |t| t.state() == SONG_PLAYING));
    let (calls, rows) = calls_and_row_events(&mut t, 100);
    assert!(
        calls >= 14 && rows > 0,
        "{calls} calls, {rows} rows after continue"
    );
}

fn arp_helper_rows(first: u32, offsets: &[i8], speed: u8) -> (Vec<u8>, Vec<u8>) {
    let mut a = song_pages_arp {
        steps: offsets.len() as u8,
        speed,
        loop_: 1,
        wave: SONG_PAGES_ARP_WAVE as u8,
        ..Default::default()
    };
    a.off[..offsets.len()].copy_from_slice(offsets);
    let mut left = [0u8; SONG_PAGES_ARP_ROWS as usize];
    let mut right = [0u8; SONG_PAGES_ARP_ROWS as usize];
    let n = unsafe { song_pages_arp_encode(&a, first, left.as_mut_ptr(), right.as_mut_ptr()) };
    (left[..n as usize].to_vec(), right[..n as usize].to_vec())
}

fn draft_song(saved: Option<&(Vec<u8>, Vec<u8>)>) -> Vec<u8> {
    let mut s = simple_spec();
    s.instr = vec![
        ins(0x09, 0x00, 1, 0, 0, 0, 0, 2, 0x09),
        ins(0x09, 0x00, 3, 0, 0, 0, 0, 2, 0x09),
    ];
    let mut wave = vec![(0x21, 0x00), (0xff, 0x00)];
    if let Some((l, r)) = saved {
        wave.extend(l.iter().copied().zip(r.iter().copied()));
    }
    s.tables[0] = wave;
    for (p, _) in &mut s.patterns {
        for c in p.iter_mut().filter(|c| c[1] != 0) {
            c[1] = 2;
        }
    }
    s.encode()
}

fn draft(t: &mut Transport, ch: u32, first: u32, rows: &(Vec<u8>, Vec<u8>)) {
    let n = rows.0.len() as u32;
    assert_eq!(
        unsafe { song_ctl_draft(&mut *t.sc, ch, first, n, rows.0.as_ptr(), rows.1.as_ptr()) },
        1
    );
}

fn voice_freq_pulse_control(t: &Transport, v: usize) -> (u32, u32, u8) {
    let o = &t.chip.osc[v];
    (o.freq, o.pw, o.control)
}

fn jam_heard(t: &mut Transport, note: u32, calls: u32) -> Vec<(u32, u32, u8)> {
    assert!(t.jam(0, note, 2));
    let mut out = Vec::new();
    let mut guard = 0;
    while out.len() < calls as usize {
        let c = t.calls();
        t.pass();
        t.block_silent();
        if t.calls() != c && (!out.is_empty() || t.chip.env[0].gate != 0) {
            out.push(voice_freq_pulse_control(t, 0));
        }
        guard += 1;
        assert!(guard < 10_000, "the jam did not sound");
    }
    out
}

fn auditioning(t: &mut Transport) {
    assert!(t.audition());
    assert!(t.run_until(400, |t| t.jam_ready()));
}

#[test]
fn a_drafts_jam_sounds_as_the_saved_rows_do() {
    let rows = arp_helper_rows(3, &[0, 4, 7, 12], 2);
    assert!(rows.0.iter().any(|&l| (1..0x10).contains(&l)), "delay rows");
    let mut saved = song_of(&draft_song(Some(&rows)), None);
    let mut t = Transport::new(&mut saved);
    auditioning(&mut t);
    let want = jam_heard(&mut t, 36, 60);
    let pitches: std::collections::BTreeSet<u32> = want.iter().map(|v| v.0).collect();
    assert!(
        pitches.len() >= 4,
        "the saved arpeggio plays its steps: {pitches:?}"
    );

    let mut blank = song_of(&draft_song(None), None);
    let before = blank.write(false);
    let mut t = Transport::new(&mut blank);
    auditioning(&mut t);
    assert_eq!(
        u32::from(t.shape().flags) & GT2_PF_WAVEDELAY,
        0,
        "the song has no delays"
    );
    draft(&mut t, 0, 3, &rows);
    assert!(t.run_until(400, |t| t.sc.draft_grow == 0 && t.sc.deriving == 0));
    assert_eq!(t.sc.draft.channel, 0);
    assert_ne!(
        u32::from(t.shape().flags) & GT2_PF_WAVEDELAY,
        0,
        "the shape widened for the draft"
    );
    assert_eq!(
        jam_heard(&mut t, 36, 60),
        want,
        "the draft plays as the saved rows"
    );
    assert!(t.jam(0, SONG_JAM_RELEASE, 2));
    assert_eq!(blank.write(false), before, "nothing written");

    let mut t2_song = song_of(&draft_song(None), None);
    let mut t2 = Transport::new(&mut t2_song);
    auditioning(&mut t2);
    let plain = jam_heard(&mut t2, 36, 30);
    assert_eq!(
        unsafe { song_ctl_draft(&mut *t.sc, 0, 0, 0, std::ptr::null(), std::ptr::null()) },
        1
    );
    t.run_until(20, |t| t.sc.draft.channel == GT2_DRAFT_OFF as u8);
    assert_eq!(t.sc.draft.channel, GT2_DRAFT_OFF as u8, "the draft is off");
    let heard = jam_heard(&mut t, 36, 30);
    assert_eq!(
        heard[1..].iter().map(|v| v.0).collect::<Vec<_>>(),
        plain[1..].iter().map(|v| v.0).collect::<Vec<_>>(),
        "without the draft the jam plays the table's rows"
    );
}

#[test]
fn a_drafts_change_is_heard_on_the_held_note() {
    let rows = arp_helper_rows(3, &[0, 0, 0, 0], 1);
    let mut blank = song_of(&draft_song(None), None);
    let mut t = Transport::new(&mut blank);
    auditioning(&mut t);
    draft(&mut t, 0, 3, &rows);
    assert!(t.run_until(400, |t| t.sc.draft_grow == 0 && t.sc.deriving == 0));
    let held = jam_heard(&mut t, 36, 10);
    let base = held.last().unwrap().0;
    assert!(held[1..].iter().all(|v| v.0 == base), "one pitch: {held:?}");
    draft(&mut t, 0, 3, &arp_helper_rows(3, &[0, 0, 12, 0], 2));
    let mut seen = Vec::new();
    for _ in 0..40 {
        let c = t.calls();
        t.pass();
        t.block_silent();
        if t.calls() != c {
            seen.push(voice_freq_pulse_control(&t, 0).0);
        }
    }
    assert!(
        seen.iter().any(|&f| f > base * 19 / 10),
        "the octave step sounds on the held note: {seen:?}"
    );
    assert!(
        seen.iter().all(|&f| f == base || f > base * 19 / 10),
        "and only the draft's steps: {seen:?}"
    );
}

#[test]
fn a_draft_plays_only_on_its_channel() {
    let saved = arp_helper_rows(3, &[0, 0, 0, 0], 1);
    let rows = arp_helper_rows(3, &[0, 7, 12, 19], 2);
    let run = |with: bool| {
        let mut song = song_of(&draft_song(Some(&saved)), None);
        let mut t = Transport::new(&mut song);
        if with {
            draft(&mut t, 0, 3, &rows);
        }
        assert!(t.play(0, 0));
        let mut out = Vec::new();
        while t.calls() < 150 {
            let c = t.calls();
            t.pass();
            t.block_silent();
            if t.calls() != c {
                out.push([
                    voice_freq_pulse_control(&t, 0),
                    voice_freq_pulse_control(&t, 1),
                    voice_freq_pulse_control(&t, 2),
                ]);
            }
        }
        out
    };
    let (plain, drafted) = (run(false), run(true));
    assert_eq!(plain.len(), drafted.len());
    for (k, (a, b)) in plain.iter().zip(&drafted).enumerate() {
        assert_eq!(
            a[1..],
            b[1..],
            "call {k}: voices 2 and 3 play the table's rows"
        );
    }
    assert!(
        plain.iter().zip(&drafted).any(|(a, b)| a[0] != b[0]),
        "voice 1 plays the draft"
    );
}
