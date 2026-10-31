// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use sidou_sys::*;
use sidou_tests::font::repo_root;
use sidou_tests::hex;
use sidou_tests::song::*;
use sidou_tests::transport::{Transport, BLOCK, SAMPLE_RATE};

fn fingerprint(b: &[u8]) -> String {
    format!("\"{}\"", hex(&Sha256::digest(b)[..8]))
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn core_js_blocks(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE as f64 / BLOCK as f64).ceil() as usize
}

fn fm1_accepted_chip_setup(song: &Song) -> Option<song_chip_setup> {
    let mut c = song_chip_setup::default();
    if unsafe { song_ext_get_chip(song.ptr(), TAG_CHPA, &mut c) } == 0 {
        return None;
    }
    let p = unsafe { sid_chip_profile_get(c.profile.into()) };
    let experts_ok = (0..usize::from(c.expert_count).min(SID_EXPERT_COUNT as usize))
        .all(|f| c.expert[f] <= SID_CHARACTER_MAX as u8 || c.expert[f] == SID_EXPERT_DEFAULT as u8);
    let ok = !p.is_null()
        && unsafe { (*p).model } == c.model
        && (c.age | c.dac | c.thump | c.bleed) & 0x80 == 0
        && c.envelope <= 1
        && u32::from(c.expert_count) <= SONG_EXPERT_MAX
        && experts_ok;
    ok.then_some(c)
}

struct ChipHooks {
    now: sid_character,
    base: sid_character,
    dac: Box<sid_dac>,
    envelope: u8,
    chip: *mut sid,
}

unsafe extern "C" fn chip_a_song(ctx: *mut c_void, _setup: *const song_chip_setup) -> i32 {
    let c = unsafe { &mut *ctx.cast::<ChipHooks>() };
    unsafe { sid_set_gate_reset(c.chip, i32::from(c.envelope == 0)) };
    1
}

unsafe extern "C" fn chip_a_character(ctx: *mut c_void, field: u32, value: i32) {
    let c = unsafe { &mut *ctx.cast::<ChipHooks>() };
    let (to, from) = match field {
        0 => (&mut c.now.age, c.base.age),
        1 => (&mut c.now.dac, c.base.dac),
        2 => (&mut c.now.thump, c.base.thump),
        3 => (&mut c.now.bleed, c.base.bleed),
        _ => return,
    };
    *to = if value < 0 { from } else { value as u8 };
    unsafe { sid_character_change(c.chip, &c.now, &mut *c.dac) };
}

struct Preview {
    t: Transport,
    _character: Box<ChipHooks>,
    slots: Box<sid_slots>,
    _pool: Box<sid_slot_pool>,
}

fn zeroed_box<T>() -> Box<T> {
    let layout = std::alloc::Layout::new::<T>();
    // SAFETY: the C structs taken here are plain data, valid all zeros.
    unsafe {
        let p = std::alloc::alloc_zeroed(layout).cast::<T>();
        assert!(!p.is_null());
        Box::from_raw(p)
    }
}

fn preview(song: &mut Song) -> Preview {
    let mut ch: sid_character = unsafe { std::mem::zeroed() };
    unsafe { sid_character_init(&mut ch) };
    let mut envelope = 1u8;
    let model = match fm1_accepted_chip_setup(song) {
        Some(c) => {
            let m = if c.model != 0 {
                SID_MODEL_8580
            } else {
                SID_MODEL_6581
            };
            ch.profile = c.profile + 1;
            (ch.age, ch.dac, ch.thump, ch.bleed) = (c.age, c.dac, c.thump, c.bleed);
            for f in 0..SID_EXPERT_COUNT {
                let v = if f < u32::from(c.expert_count) {
                    c.expert[f as usize].into()
                } else {
                    SID_EXPERT_DEFAULT
                };
                unsafe { sid_character_expert_set(&mut ch, m, f, v) };
            }
            envelope = c.envelope;
            m
        }
        None => {
            let mut o = gt2_options::default();
            unsafe { song_ext_get_options(song.ptr(), &mut o) };
            if o.sid_model != 0 {
                SID_MODEL_8580
            } else {
                SID_MODEL_6581
            }
        }
    };
    let mut t = Transport::new(song);
    let mut character = Box::new(ChipHooks {
        now: ch,
        base: ch,
        dac: Box::new(unsafe { std::mem::zeroed() }),
        envelope,
        chip: &mut *t.chip,
    });
    unsafe {
        sid_init(&mut *t.chip, model, SID_CLOCK_PAL, SAMPLE_RATE);
        sid_character_apply(&mut *t.chip, &character.now, &mut *character.dac);
        let hooks = song_chip_hooks {
            chip_a_song: Some(chip_a_song),
            chip_a_keys: None,
            chip_a_character: Some(chip_a_character),
            ctx: (&mut *character as *mut ChipHooks).cast(),
        };
        song_ctl_set_hooks(&mut *t.sc, &hooks);
    }
    let mut pool: Box<sid_slot_pool> = zeroed_box();
    let mut slots: Box<sid_slots> = zeroed_box();
    unsafe { sid_slots_init(&mut *slots, SAMPLE_RATE, 0x51d0, &mut *pool) };
    let mut v = song_slots::default();
    if unsafe { song_ext_get_slots(song.ptr(), &mut v) } != 0 {
        for k in 0..usize::from(v.count).min(SID_SLOT_COUNT as usize) {
            let f = |i: usize| f32::from(v.slot[k][i]) * (1.0 / 127.0);
            let p = sid_slot_params {
                algorithm: v.slot[k][0].into(),
                amount: f(1),
                a: f(2),
                b: f(3),
                mix: f(4),
            };
            unsafe { sid_slots_set(&mut *slots, k as u32, &p) };
        }
        let silence = [0i16; BLOCK];
        let mut out = [0i16; 2 * BLOCK];
        for _ in 0..2 * 345 {
            unsafe {
                sid_slots_process(
                    &mut *slots,
                    silence.as_ptr(),
                    out.as_mut_ptr(),
                    BLOCK as u32,
                )
            };
        }
    }
    Preview {
        t,
        _character: character,
        slots,
        _pool: pool,
    }
}

fn render(t: &mut Transport, n: usize, out: &mut Vec<u8>) {
    let mut b = [0i16; BLOCK];
    for _ in 0..n {
        t.pass();
        t.block(&mut b);
        out.extend(b.iter().flat_map(|s| s.to_le_bytes()));
    }
}

fn two_seconds_of_song_pcm(song: &mut Song) -> Option<Vec<u8>> {
    let mut p = preview(song);
    let t = &mut p.t;
    if !t.play(0, 0) {
        return None;
    }
    let mut out = Vec::new();
    render(t, core_js_blocks(2.0), &mut out);
    Some(out)
}

fn instrument_at_note_48_pcm(song: &mut Song) -> Option<Vec<u8>> {
    let mut p = preview(song);
    let t = &mut p.t;
    if !t.audition() {
        return None;
    }
    let mut b = [0i16; BLOCK];
    for _ in 0..1000 {
        if t.jam_ready() {
            break;
        }
        t.pass();
        t.block(&mut b);
    }
    if !t.jam_ready() || !t.jam(0, 48, 1) {
        return None;
    }
    t.pass();
    let mut out = Vec::new();
    render(t, core_js_blocks(0.5), &mut out);
    t.jam(0, 0xff, 0);
    t.pass();
    render(t, core_js_blocks(1.0), &mut out);
    Some(out)
}

fn gti5(song: &Song, i: u32) -> Result<Vec<u8>, u32> {
    let mut ex = gt2_ins_export::default();
    let e = unsafe { gt2_ins_export_begin(&mut ex, song.ptr(), i) };
    if e != GT2_OK {
        return Err(e as u32);
    }
    let mut out = vec![0u8; ex.length as usize];
    let n = unsafe {
        gt2_ins_export_read(
            (&mut ex as *mut gt2_ins_export).cast(),
            0,
            out.as_mut_ptr(),
            ex.length,
        )
    };
    assert_eq!(n, ex.length);
    Ok(out)
}

fn entry(name: &str, file: &[u8], embed: bool) -> String {
    let mut o = String::new();
    let _ = write!(o, "{{\"name\": {}", json_str(name));
    if embed {
        let _ = write!(o, ", \"hex\": \"{}\"", hex(file));
    }
    let mut s = Song::device();
    let r = s.read(file, 4);
    let (result, info) = match &r {
        Ok(i) => (0, i),
        Err(i) => (-1, i),
    };
    let _ = write!(
        o,
        ", \"read\": {{\"ok\": {}, \"message\": {}, \"version\": {}, \"channels\": {}, \"trailer\": {}}}",
        result == 0,
        json_str(&info.msg),
        info.version,
        info.channels,
        info.trailer
    );
    if r.is_err() {
        o.push('}');
        return o;
    }
    let sng = s.write(true);
    let plain = s.write(false);
    let (check, warning) = s.check_plain(&plain);
    let goattracker = gt2_ref::canonical(&plain).is_ok_and(|c| c == plain);
    let _ = write!(
        o,
        ",\n   \"sng\": [{}, {}], \"plain\": [{}, {}], \"check\": [{}, {}], \"goattracker\": {}",
        sng.len(),
        fingerprint(&sng),
        plain.len(),
        fingerprint(&plain),
        check,
        json_str(&warning),
        goattracker
    );
    let n = unsafe { gt2_saved_instruments(s.ptr()) };
    let ins: Vec<String> = (1..=n)
        .map(|i| match gti5(&s, i) {
            Ok(b) => format!("[{}, {}]", b.len(), fingerprint(&b)),
            Err(e) => format!("[-{e}]"),
        })
        .collect();
    let _ = write!(o, ",\n   \"gti5\": [{}]", ins.join(", "));
    let pcm = |p: Option<Vec<u8>>| {
        p.map_or("null".into(), |b| {
            format!("[{}, {}]", b.len() / 2, fingerprint(&b))
        })
    };
    let song_audio = pcm(two_seconds_of_song_pcm(&mut s));
    let ins_audio = if n >= 1 {
        pcm(instrument_at_note_48_pcm(&mut s))
    } else {
        "null".into()
    };
    let _ = write!(
        o,
        ",\n   \"song_pcm\": {song_audio}, \"instrument_pcm\": {ins_audio}}}"
    );
    o
}

fn ambiguous_mono_and_stereo() -> (Vec<u8>, Vec<u8>) {
    let lookalike = [0xff, 0x00, 0x01, 0xff, 0x00, 0x01, 0xff, 0x00];
    let mut rng = Rng(0xdf75_5aa4_46c7_9a0d);
    let mut spec = random_canonical(&mut rng, false);
    let mut i = [0u8; 25];
    i[..8].copy_from_slice(&lookalike);
    for b in i.iter_mut().skip(8) {
        *b = if rng.chance(50) { 0 } else { rng.byte() };
    }
    i[0] |= 1;
    spec.orders.truncate(1);
    spec.instr = vec![i];
    for p in spec.patterns.iter_mut() {
        for r in p.0.iter_mut() {
            r[1] = r[1].min(1);
        }
    }
    let file = spec.encode();
    let mut s = Song::device();
    let info = s.read(&file, 4).unwrap();
    assert_eq!(info.channels, 6);
    let mut m = Song::device();
    set_mono_from_spec(&mut m, &spec);
    assert_eq!(m.write(false), file);
    (m.write(true), s.write(true))
}

const OP_INSTR: u32 = 0;
const OP_TABLE: u32 = 1;
const OP_ROW: u32 = 2;
const OP_ORDER: u32 = 3;
const OP_ORDER_INSERT: u32 = 4;
const OP_ORDER_DELETE: u32 = 5;
const OP_PUSH_ROW: u32 = 6;
const OP_PULL_ROW: u32 = 7;
const OP_INSERT_ROW: u32 = 8;
const OP_DELETE_ROW: u32 = 9;
const OP_RESIZE: u32 = 10;
const OP_PATTERN_ADD: u32 = 11;
const OP_PATTERN_REMOVE: u32 = 12;

fn primitive(
    s: &mut Song,
    [op, a, b, c, d]: [u32; 5],
    added: &mut u32,
    e: &mut gt2_edit,
) -> gt2_err {
    let cell = d.to_le_bytes();
    let p = s.ptr_mut();
    unsafe {
        match op {
            OP_INSTR => gt2_set_instr(p, a, b, c as u8, e),
            OP_TABLE => gt2_set_table(p, a, b, c, d as u8, e),
            OP_ROW => gt2_set_row(p, a, b, cell.as_ptr(), e),
            OP_ORDER => gt2_set_order(p, a, b, c, d as u8, e),
            OP_ORDER_INSERT => gt2_order_insert(p, a, b, c, d as u8, e),
            OP_ORDER_DELETE => gt2_order_delete(p, a, b, c, e),
            OP_PUSH_ROW => gt2_pattern_push_row(p, a, b, e),
            OP_PULL_ROW => gt2_pattern_pull_row(p, a, b, e),
            OP_INSERT_ROW => gt2_pattern_insert_row(p, a, b, e),
            OP_DELETE_ROW => gt2_pattern_delete_row(p, a, b, e),
            OP_RESIZE => gt2_pattern_resize(p, a, b, e),
            OP_PATTERN_ADD => gt2_pattern_add(p, a, added, e),
            OP_PATTERN_REMOVE => gt2_pattern_remove(p, a, e),
            _ => GT2_ERR_RANGE,
        }
    }
}

fn edit(s: &mut Song, op: [u32; 5]) -> (u32, u32) {
    let mut added = 0;
    loop {
        let mut e = gt2_edit::default();
        let r = primitive(s, op, &mut added, &mut e);
        if r == GT2_ERR_FRAGMENTED {
            let mut m = gt2_edit::default();
            if unsafe { gt2_compact_step(s.ptr_mut(), &mut m) } <= 0 {
                return (r, added);
            }
            unsafe { gt2_edit_commit(s.ptr_mut(), &m) };
            continue;
        }
        if r == GT2_OK {
            unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
        }
        return (r, added);
    }
}

fn comet_with_locks_slots_and_character() -> (Vec<u8>, u32) {
    let comet = std::fs::read(repo_root().join("assets/songs/chrome-comet.sng")).unwrap();
    let mut s = Song::device();
    s.read(&comet, 4).unwrap();
    let first = s
        .order(0, 0)
        .unwrap()
        .0
        .into_iter()
        .find(|&e| e < 0xd0)
        .unwrap() as u32;
    for (row, param, value) in [
        (4, SONG_LOCK_CUTOFF, 900),
        (4, SONG_LOCK_AGE, 100),
        (10, SONG_LOCK_PULSE, 2048),
        (20, SONG_LOCK_CUTOFF, 200),
    ] {
        assert_eq!(
            unsafe { song_lock_put(s.ptr_mut(), first, row, param, value) },
            0
        );
    }
    let mut v = song_slots {
        count: 3,
        ..Default::default()
    };
    v.slot[1] = [1, 80, 40, 20, 64];
    v.slot[2] = [1, 70, 50, 40, 50];
    assert_eq!(unsafe { song_ext_set_slots(s.ptr_mut(), &v) }, 0);
    let mut c = song_chip_setup::default();
    assert_ne!(unsafe { song_ext_get_chip(s.ptr(), TAG_CHPA, &mut c) }, 0);
    (c.age, c.dac, c.bleed) = (40, 30, 20);
    assert_eq!(unsafe { song_ext_set_chip(s.ptr_mut(), TAG_CHPA, &c) }, 0);
    let file = s.write(true);
    let mut back = Song::device();
    back.read(&file, 4).unwrap();
    assert_eq!(back.write(true), file);
    (file, first)
}

fn editor_script(p: u32) -> Vec<[u32; 5]> {
    let cell = |b: [u8; 4]| u32::from_le_bytes(b);
    vec![
        [OP_INSTR, 1, 0, 0x0a, 0],
        [OP_INSTR, 1, 9, u32::from(b'X'), 0],
        [OP_INSTR, 13, 0, 0x11, 0],
        [OP_INSTR, 64, 0, 1, 0],
        [OP_INSTR, 1, 25, 1, 0],
        [OP_TABLE, 0, 0, 1, 0x80],
        [OP_TABLE, 3, 200, 0, 1],
        [OP_TABLE, 0, 255, 0, 1],
        [OP_ROW, p, 0, 0, cell([0x60 + 48, 1, 0, 0])],
        [OP_ROW, p, 1, 0, cell([0xbe, 0, 4, 0x22])],
        [OP_ROW, p, 3, 0, cell([0x60 + 50, 2, 1, 0x10])],
        [OP_ROW, p, 200, 0, cell([0xbd, 0, 0, 0])],
        [OP_PUSH_ROW, p, 2, 0, 0],
        [OP_PULL_ROW, p, 0, 0, 0],
        [OP_INSERT_ROW, p, 5, 0, 0],
        [OP_DELETE_ROW, p, 5, 0, 0],
        [OP_RESIZE, p, 16, 0, 0],
        [OP_RESIZE, p, 129, 0, 0],
        [OP_ORDER, 0, 0, 0, 1],
        [OP_ORDER_INSERT, 0, 0, 0, 0xf2],
        [OP_ORDER_INSERT, 0, 0, 0, 0xd3],
        [OP_ORDER_INSERT, 0, 0, 1, 0xf1],
        [OP_ORDER_DELETE, 0, 0, 0, 0],
        [OP_ORDER_DELETE, 0, 0, 0, 0],
        [OP_PATTERN_ADD, 64, 0, 0, 0],
        [OP_PATTERN_ADD, 0, 0, 0, 0],
        [OP_PATTERN_REMOVE, 1, 0, 0, 0],
        [OP_PATTERN_REMOVE, 200, 0, 0, 0],
        [OP_ORDER, 0, 1, 0, 51],
        [OP_PATTERN_REMOVE, 51, 0, 0, 0],
        [13, 0, 0, 0, 0],
    ]
}

fn save_size_refusals(file: &[u8]) -> (u32, u32, u32, u32, Vec<u8>) {
    let add = |s: &mut Song, rows: u32| -> (u32, u32) {
        let mut k = 0;
        loop {
            let (r, p) = edit(s, [OP_PATTERN_ADD, rows, 0, 0, 0]);
            if r != GT2_OK || p == 207 {
                return (k + u32::from(r == GT2_OK), r);
            }
            k += 1;
        }
    };
    let mut s = Song::device();
    s.read(file, 4).unwrap();
    let (arena_count, arena) = add(&mut s, 128);
    let mut s = Song::device();
    s.read(file, 4).unwrap();
    assert_eq!(add(&mut s, 1).1, GT2_OK);
    assert_eq!(edit(&mut s, [OP_ORDER, 0, 0, 0, 207]).0, GT2_OK);
    for p in 51..207 {
        assert_eq!(edit(&mut s, [OP_PATTERN_REMOVE, p, 0, 0, 0]).0, GT2_OK);
    }
    let (save_count, save) = add(&mut s, 128);
    (arena_count, arena, save_count, save, s.write(true))
}

fn stereo_song_pcm_from_position(song: &mut Song, pos: u32) -> Vec<u8> {
    let mut p = preview(song);
    assert!(p.t.play(0, pos));
    let mut out = Vec::new();
    let mut b = [0i16; BLOCK];
    let mut lr = [0i16; 2 * BLOCK];
    for _ in 0..core_js_blocks(2.0) {
        p.t.pass();
        p.t.block(&mut b);
        unsafe { sid_slots_process(&mut *p.slots, b.as_ptr(), lr.as_mut_ptr(), BLOCK as u32) };
        out.extend(lr.iter().flat_map(|s| s.to_le_bytes()));
    }
    out
}

fn pattern_pcm(song: &Song, pattern: u32, channel: u32) -> Vec<u8> {
    let mut s = Song::device();
    s.copy_from(song);
    for ch in 0..3 {
        let mut e = gt2_edit::default();
        let entry = pattern as u8;
        let r = unsafe { gt2_order_replace(s.ptr_mut(), 0, ch, &entry, 1, 0, &mut e) };
        assert_eq!(r, GT2_OK);
        unsafe { gt2_edit_commit(s.ptr_mut(), &e) };
    }
    let mut p = preview(&mut s);
    assert!(p.t.play(0, 0));
    assert!(p.t.mute(7 & !(1 << channel)));
    let mut out = Vec::new();
    render(&mut p.t, core_js_blocks(2.0), &mut out);
    out
}

fn c_text(buf: &[std::os::raw::c_char]) -> String {
    buf.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect()
}

fn device_words() -> String {
    let mut cells = Vec::new();
    for note in [0x60u8, 0x6d, 0x90, 0xbc, 0xbd, 0xbe, 0xbf] {
        for (ins, cmd, par) in [
            (0u8, 0u8, 0u8),
            (1, 0, 0x12),
            (0x3f, 4, 0x2a),
            (0, 0xf, 0xff),
        ] {
            let mut out = [0 as std::os::raw::c_char; 9];
            let c = [note, ins, cmd, par];
            unsafe { tracker_view_cell(c.as_ptr(), out.as_mut_ptr()) };
            cells.push(format!("[\"{}\", {}]", hex(&c), json_str(&c_text(&out))));
        }
    }
    let mut orders = Vec::new();
    for e in (0u8..=0xfe).step_by(1) {
        let mut out = [0 as std::os::raw::c_char; 4];
        unsafe { tracker_view_order_entry(e, out.as_mut_ptr()) };
        orders.push(json_str(&c_text(&out)));
    }
    let mut meanings = Vec::new();
    for t in 0..4u32 {
        for l in [
            0x00u8, 0x01, 0x0f, 0x10, 0x41, 0x7f, 0x80, 0x81, 0xef, 0xf0, 0xfe, 0xff,
        ] {
            for r in [0x00u8, 0x01, 0x40, 0x7f, 0x80, 0x81, 0x83, 0xdf, 0xe0, 0xff] {
                let mut out = [0 as std::os::raw::c_char; 31];
                unsafe { song_pages_table_meaning(t, l, r, out.as_mut_ptr()) };
                meanings.push(format!("[{t}, {l}, {r}, {}]", json_str(&c_text(&out))));
            }
        }
    }
    format!(
        "\"cells\": [{}],\n   \"orders\": [{}],\n   \"meanings\": [{}]",
        cells.join(", "),
        orders.join(", "),
        meanings.join(", ")
    )
}

fn editor() -> String {
    let (file, p) = comet_with_locks_slots_and_character();
    let mut s = Song::device();
    s.read(&file, 4).unwrap();
    let steps: Vec<String> = editor_script(p)
        .into_iter()
        .map(|op| {
            let (r, added) = edit(&mut s, op);
            let sng = s.write(true);
            format!(
                "[{:?}, {r}, {added}, {}, {}, {}]",
                op,
                sng.len(),
                fingerprint(&sng),
                unsafe { gt2_saved_size(s.ptr()) }
            )
        })
        .collect();
    let (arena_count, arena, save_count, save, full) = save_size_refusals(&file);
    let mut plus = Song::device();
    plus.read(&file, 4).unwrap();
    let out = stereo_song_pcm_from_position(&mut plus, 2);
    let pat = pattern_pcm(&plus, p, 1);
    let ins = instrument_at_note_48_pcm(&mut plus).unwrap();
    let pcm = |b: &[u8], ch: usize| format!("[{}, {}]", b.len() / 2 / ch, fingerprint(b));
    format!(
        "{{\"song\": \"{}\", \"pattern\": {p},\n   \"steps\": [\n    {}],\n   \"fill\": [{arena_count}, {arena}, {save_count}, {save}, {}, {}],\n   \"out_pcm\": {}, \"pattern_pcm\": {}, \"instrument_pcm\": {},\n   {}}}",
        hex(&file),
        steps.join(",\n    "),
        full.len(),
        fingerprint(&full),
        pcm(&out, 2),
        pcm(&pat, 1),
        pcm(&ins, 1),
        device_words()
    )
}

#[test]
fn web_core_fixture() {
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(repo_root().join("assets/songs"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sng"))
        .map(|p| {
            (
                format!("assets/songs/{}", p.file_name().unwrap().to_string_lossy()),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    files.sort();
    let mut examples = gt2_ref::examples();
    examples.sort();
    for (name, b) in examples {
        files.push((format!("third_party/goattracker2/examples/{name}"), b));
    }
    let mut entries: Vec<String> = files.iter().map(|(n, b)| entry(n, b, false)).collect();
    let comet = std::fs::read(repo_root().join("assets/songs/chrome-comet.sng")).unwrap();
    for cut in [3usize, 300, 2000] {
        entries.push(entry(
            &format!("chrome-comet cut to {cut}"),
            &comet[..cut],
            true,
        ));
    }
    entries.push(entry(
        "not a song",
        b"PSID not a song at all, just text",
        true,
    ));
    let (mono, stereo) = ambiguous_mono_and_stereo();
    entries.push(entry("ambiguous, mono", &mono, true));
    entries.push(entry("ambiguous, stereo", &stereo, true));
    let json = format!(
        "{{\"songs\": [\n  {}\n],\n \"editor\": {}}}\n",
        entries.join(",\n  "),
        editor()
    );
    let path = repo_root().join("web/core-fixtures.json");
    if std::env::var_os("SIDOU_WEB_CORE_FIXTURE_UPDATE").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == json,
        "web/core-fixtures.json is not what this build makes: rerun with SIDOU_WEB_CORE_FIXTURE_UPDATE=1"
    );
}
