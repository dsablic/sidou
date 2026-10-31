// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;

pub const SAMPLE_RATE: u32 = 44_118;
pub const HALF_FRAMES: usize = 128;
pub const POT: u16 = 900;
const SLOTS_SEED: u32 = 0x51D0;

const TUNE_PASSES_PER_BLOCK: usize = 400;
const TUNE_INIT_PASSES: usize = 4000;

pub fn boot_gain() -> f32 {
    let g = unsafe { master_gain_of(u32::from(POT), LIVE_MASTER_MAX) };
    g as f32 / MASTER_UNITY as f32
}

fn master_stage(half: &mut [i16], master: u16) {
    let mut block = [0i16; 2 * HALF_FRAMES];
    let n = half.len();
    assert!(n <= block.len());
    block[..n].copy_from_slice(half);
    unsafe {
        let g = master_gain_of(u32::from(master), LIVE_MASTER_MAX);
        master_gain_apply(block.as_mut_ptr(), MASTER_BLOCK_LOG2, g, g);
    }
    half.copy_from_slice(&block[..n]);
}

fn tail_stage(gate: &mut tail_gate, a: &sid, b: Option<&sid>, mono: &mut [i16], n: usize) {
    let whole_block = n == HALF_FRAMES;
    if whole_block {
        unsafe {
            tail_gate_run(
                gate,
                a,
                b.map_or(std::ptr::null(), |b| b as *const _),
                mono.as_mut_ptr(),
            )
        };
    }
}

fn thump_stage(t: &mut thump_out, a: &mut sid, b: Option<&mut sid>, half: &mut [i16], n: usize) {
    unsafe {
        thump_out_take(t, 0, a);
        if let Some(b) = b {
            thump_out_take(t, 1, b);
        }
        thump_out_apply(t, half.as_mut_ptr(), n as u32);
    }
}

pub struct HostInstrument {
    pub chip: Box<sid>,
    _spare: Box<sid>,
    _dacs: Box<[sid_dac; 2]>,
    _pool: Box<sid_slot_pool>,
    slots: Box<sid_slots>,
    _fx: Box<fx_live>,
    pub target: Box<fx_live_target>,
    pub live: Box<live>,
    gate: tail_gate,
    thump: thump_out,
    pub mono: Vec<i16>,
}

impl HostInstrument {
    pub fn new() -> Self {
        let mut inst = Self::unwired();
        let target: *mut fx_live_target = &mut *inst.target;
        inst.wire(Some(fx_live_sid_sink), target.cast());
        inst
    }

    pub fn unwired() -> Self {
        let mut chip = Box::<sid>::default();
        let mut spare = Box::<sid>::default();
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut pool = Box::<sid_slot_pool>::default();
        let mut slots = Box::<sid_slots>::default();
        let mut fx = Box::<fx_live>::default();
        let target = Box::new(fx_live_target {
            sid: live_sid_target {
                chip: &mut *chip,
                clock_hz: SID_CLOCK_PAL,
                sample_rate: SAMPLE_RATE,
                dacs: dacs.as_mut_ptr(),
                spare: &mut *spare,
                ..Default::default()
            },
            fx: &mut *fx,
        });
        unsafe {
            sid_slots_init(&mut *slots, SAMPLE_RATE, SLOTS_SEED, &mut *pool);
            fx_live_init(&mut *fx, &mut *slots);
        }
        HostInstrument {
            chip,
            _spare: spare,
            _dacs: dacs,
            _pool: pool,
            slots,
            _fx: fx,
            target,
            live: Box::<live>::default(),
            gate: tail_gate::default(),
            thump: thump_out::default(),
            mono: vec![0i16; HALF_FRAMES],
        }
    }

    pub fn wire(&mut self, sink: live_sink_fn, ctx: *mut std::ffi::c_void) {
        unsafe {
            live_init(&mut *self.live, sink, ctx, SID_MODEL_6581);
            let sid_target: *mut live_sid_target = &mut self.target.sid;
            live_set_preparer(&mut *self.live, Some(live_sid_prepare), sid_target.cast());
            live_set_master(&mut *self.live, POT);
        }
    }

    pub fn render_alone(&mut self, half: &mut [i16]) {
        let n = half.len() / 2;
        unsafe {
            sid_render(&mut *self.chip, self.mono.as_mut_ptr(), n as u32);
            tail_stage(&mut self.gate, &self.chip, None, &mut self.mono, n);
            sid_slots_process(
                &mut *self.slots,
                self.mono.as_ptr(),
                half.as_mut_ptr(),
                n as u32,
            );
        }
        thump_stage(&mut self.thump, &mut self.chip, None, half, n);
        master_stage(half, POT);
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn finish_with_chip_b(
        &mut self,
        a: *mut sid,
        b: *mut sid,
        b_rendered: bool,
        over: &[i16],
        half: &mut [i16],
        master: u16,
        before_slots: impl FnOnce(&mut [i16]),
    ) {
        let n = half.len() / 2;
        unsafe {
            let b_ref = b_rendered.then_some(&*b);
            tail_stage(&mut self.gate, &*a, b_ref, &mut self.mono, n);
            before_slots(&mut self.mono);
            let wide = if b_rendered {
                over.as_ptr()
            } else {
                std::ptr::null()
            };
            sid_slots_process_wide(
                &mut *self.slots,
                self.mono.as_ptr(),
                wide,
                half.as_mut_ptr(),
                n as u32,
            );
            if !b_rendered {
                thump_out_drop(b);
            }
            thump_stage(
                &mut self.thump,
                &mut *a,
                b_rendered.then_some(&mut *b),
                half,
                n,
            );
        }
        master_stage(half, master);
    }
}

pub fn render_instrument(
    frames: usize,
    mut before_block: impl FnMut(usize, &mut live),
) -> Vec<i16> {
    let mut inst = HostInstrument::new();
    let mut out = vec![0i16; 2 * frames];
    for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
        before_block(i, &mut inst.live);
        inst.render_alone(half);
    }
    out
}

pub fn render_host(
    frames: usize,
    note: u8,
    note_block: usize,
    ccs: &[(usize, u8, u8)],
) -> Vec<i16> {
    render_instrument(frames, |i, l| unsafe {
        for &(_, cc, value) in ccs.iter().filter(|c| c.0 == i) {
            live_cc(l, cc, value);
        }
        if i == note_block {
            live_note_on(l, note, 100);
        }
    })
}

unsafe fn set_up_chip(target: *mut live_sid_target, profile: u32, model: u32) {
    let sink =
        |reg: u32, value: u32| unsafe { live_sid_sink(target.cast(), reg as u8, value as u8) };
    sink(LIVE_REG_CHAR_FIRST, profile);
    for f in 1..LIVE_CHAR_FIELDS {
        sink(LIVE_REG_CHAR_FIRST + f, 0);
    }
    sink(LIVE_REG_RESET, model);
}

struct HostChipB {
    chip: Box<sid>,
    _dacs: Box<[sid_dac; 2]>,
    target: Box<live_sid_target>,
    mono: Vec<i16>,
    over: Vec<i16>,
    on: bool,
    wanted: bool,
}

impl HostChipB {
    fn with_keys_setup() -> Self {
        let mut chip = Box::<sid>::default();
        let mut dacs = Box::<[sid_dac; 2]>::default();
        let mut target = Box::new(live_sid_target {
            chip: &mut *chip,
            clock_hz: SID_CLOCK_PAL,
            sample_rate: SAMPLE_RATE,
            dacs: dacs.as_mut_ptr(),
            ..Default::default()
        });
        let t: *mut live_sid_target = &mut *target;
        unsafe {
            set_up_chip(t, SID_CHIP_6581_R4AR, SID_MODEL_6581);
            reg_ring_power_on((*t).chip);
            live_sid_sink(t.cast(), LIVE_REG_GATE_RESET as u8, 1);
        }
        HostChipB {
            chip,
            _dacs: dacs,
            target,
            mono: vec![0i16; HALF_FRAMES],
            over: vec![0i16; HALF_FRAMES],
            on: false,
            wanted: false,
        }
    }

    fn rewrite_keys_onto(&mut self, l: &mut live) {
        let (sink, ctx) = (l.sink, l.sink_ctx);
        l.sink = Some(live_sid_sink);
        l.sink_ctx = (&mut *self.target as *mut live_sid_target).cast();
        unsafe { live_rewrite(l) };
        l.sink = sink;
        l.sink_ctx = ctx;
    }

    fn in_use(&self, chip: &sid) -> (bool, bool) {
        let tails = self.on && chip.env.iter().any(|e| e.counter != 0);
        let b_in = self.wanted || tails;
        (b_in, b_in || self.on)
    }

    fn mix_into(&mut self, mono: &mut [i16], n: usize, b_in: bool) {
        let fade = if b_in == self.on {
            CHIP_MIX_FULL
        } else if b_in {
            CHIP_MIX_IN
        } else {
            CHIP_MIX_OUT
        };
        let past_int16 = unsafe {
            chip_mix(
                mono.as_mut_ptr(),
                self.mono.as_ptr(),
                self.over.as_mut_ptr(),
                n as u32,
                fade,
            )
        };
        if past_int16 == 0 {
            self.over.fill(0);
        }
        self.on = b_in;
    }
}

pub struct ChipEvent {
    pub block: usize,
    pub a: Vec<(u8, u8)>,
    pub b: Vec<(u8, u8)>,
    pub b_use: Option<bool>,
}

pub struct ChipsRender {
    pub out: Vec<i16>,
    pub a: Vec<i16>,
    pub b: Vec<i16>,
}

pub fn render_host_chips(frames: usize, events: &[ChipEvent]) -> ChipsRender {
    let mut inst = HostInstrument::new();
    let mut b = HostChipB::with_keys_setup();
    let mut r = ChipsRender {
        out: vec![0i16; 2 * frames],
        a: vec![0i16; frames],
        b: vec![0i16; frames],
    };
    for (i, half) in r.out.chunks_mut(2 * HALF_FRAMES).enumerate() {
        for e in events.iter().filter(|e| e.block == i) {
            for &(reg, v) in &e.a {
                unsafe { sid_write(&mut *inst.chip, reg, v) };
            }
            for &(reg, v) in &e.b {
                unsafe { sid_write(&mut *b.chip, reg, v) };
            }
            if let Some(u) = e.b_use {
                b.wanted = u;
            }
        }
        let n = half.len() / 2;
        let block = i * HALF_FRAMES..i * HALF_FRAMES + n;
        unsafe { sid_render(&mut *inst.chip, inst.mono.as_mut_ptr(), n as u32) };
        r.a[block.clone()].copy_from_slice(&inst.mono[..n]);
        let (b_in, b_rendered) = b.in_use(&b.chip);
        if b_rendered {
            unsafe { sid_render(&mut *b.chip, b.mono.as_mut_ptr(), n as u32) };
            r.b[block].copy_from_slice(&b.mono[..n]);
            b.mix_into(&mut inst.mono, n, b_in);
        }
        let a: *mut sid = &mut *inst.chip;
        unsafe { inst.finish_with_chip_b(a, &mut *b.chip, b_rendered, &b.over, half, POT, |_| {}) };
    }
    r
}

pub fn device_song(sng: &[u8], o: Option<&gt2_options>) -> sidou_tests::song::Song {
    let mut song = sidou_tests::song::Song::device();
    song.read(sng, 0).expect("the song reads");
    if let Some(o) = o {
        assert_eq!(unsafe { song_ext_set_options(song.ptr_mut(), o) }, 0);
    }
    song
}

pub type SongPrim = Box<dyn Fn(*mut gt2_song, &mut gt2_edit) -> u32>;

pub struct SongPlay<'a> {
    pub song: &'a sidou_tests::song::Song,
    pub play_block: usize,
    pub ccs: &'a [(usize, u8, u8)],
    pub ticks: &'a [u32],
    pub edits: &'a [(usize, SongPrim)],
    pub stop_block: Option<usize>,
    pub b_use_block: Option<usize>,
    pub b_rewrite_block: Option<usize>,
}

struct HostSink {
    target: *mut fx_live_target,
    reset: bool,
}

unsafe extern "C" fn host_song_sink(ctx: *mut std::ffi::c_void, reg: u8, value: u8) {
    let h = unsafe { &mut *(ctx as *mut HostSink) };
    if reg == LIVE_REG_RESET as u8 {
        h.reset = true;
    }
    unsafe { fx_live_sid_sink(h.target.cast(), reg, value) };
}

unsafe extern "C" fn host_ready(
    _ctx: *mut std::ffi::c_void,
    _setup: *const song_chip_setup,
) -> i32 {
    1
}

pub fn render_host_song(frames: usize, p: &SongPlay) -> Vec<i16> {
    render_host_song_master(frames, p, POT)
}

pub fn render_host_song_master(frames: usize, p: &SongPlay, master: u16) -> Vec<i16> {
    let mut inst = HostInstrument::unwired();
    let mut sink = Box::new(HostSink {
        target: &mut *inst.target,
        reset: false,
    });
    let sink_ctx: *mut HostSink = &mut *sink;
    inst.wire(Some(host_song_sink), sink_ctx.cast());
    let mut song = sidou_tests::song::Song::device();
    song.copy_from(p.song);
    let mut sc = Box::<song_ctl>::default();
    let mut shapes = Box::<[gt2_shape; 2]>::default();
    let mut work = Box::<song_work>::default();
    let mut out = vec![0i16; 2 * frames];
    unsafe {
        let (a, b) = shapes.split_at_mut(1);
        song_ctl_init(&mut *sc, song.ptr_mut(), &mut a[0], &mut b[0], &mut *work);
        let hooks = song_chip_hooks {
            chip_a_song: Some(host_ready),
            chip_a_keys: None,
            chip_a_character: None,
            ctx: std::ptr::null_mut(),
        };
        song_ctl_set_hooks(&mut *sc, &hooks);
    }
    let mut b = HostChipB::with_keys_setup();
    sink.reset = false;
    for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
        let l = &mut *inst.live;
        unsafe {
            for &(_, cc, value) in p.ccs.iter().filter(|c| c.0 == i) {
                live_cc(l, cc, value);
            }
            for _ in 0..p.ticks.get(i).copied().unwrap_or(0) {
                live_tick(l);
            }
        }
        if p.b_rewrite_block == Some(i) {
            b.rewrite_keys_onto(l);
        }
        if p.b_use_block == Some(i) {
            b.wanted = true;
        }
        unsafe {
            if i == p.play_block {
                sid_set_gate_reset(&mut *inst.chip, 0);
                assert_eq!(song_ctl_play(&mut *sc, 0, 0), 1);
                while song_ctl_state(&*sc) == SONG_LOADING {
                    song_ctl_poll(&mut *sc, SONG_POLL_SLICE | SONG_POLL_RING);
                }
            }
            if p.edits.iter().any(|e| e.0 == i) {
                song_ctl_poll(&mut *sc, SONG_POLL_RING);
            }
            for (_, prim) in p.edits.iter().filter(|e| e.0 == i) {
                let mut e = gt2_edit::default();
                assert_eq!(prim(song.ptr_mut(), &mut e), GT2_OK, "the host's edit");
                assert_eq!(song_ctl_edit(&mut *sc, &e), 1, "the host's edit went");
                while sc.deriving != 0 {
                    song_ctl_poll(&mut *sc, SONG_POLL_SLICE | SONG_POLL_RING);
                }
            }
            if p.stop_block == Some(i) {
                assert_eq!(song_ctl_stop(&mut *sc), 1);
            }
            song_ctl_poll(&mut *sc, SONG_POLL_RING);
            song_ctl_commit(&mut *sc);
            if std::mem::take(&mut sink.reset) {
                song_ctl_chip_swapped((&mut *sc as *mut song_ctl).cast(), 0, &mut *inst.chip);
            }
            let n = half.len() / 2;
            song_ctl_block(&mut *sc, &mut *inst.chip);
            song_ctl_render(&mut *sc, &mut *inst.chip, inst.mono.as_mut_ptr(), n as u32);
            let (b_in, b_rendered) = b.in_use(&b.chip);
            if b_rendered {
                sid_render(&mut *b.chip, b.mono.as_mut_ptr(), n as u32);
                b.mix_into(&mut inst.mono, n, b_in);
            }
            let a: *mut sid = &mut *inst.chip;
            let sc: *mut song_ctl = &mut *sc;
            inst.finish_with_chip_b(a, &mut *b.chip, b_rendered, &b.over, half, master, |mono| {
                song_ctl_block_end(sc, mono.as_mut_ptr(), n as u32)
            });
        }
    }
    out
}

pub struct PsidPlay<'a> {
    pub file: &'a [u8],
    pub song: u32,
    pub psid_block: usize,
    pub fresh_block: [Option<usize>; 2],
    pub start_block: usize,
    pub b_use_block: Option<usize>,
    pub b_rewrite_block: Option<usize>,
    pub mute_block: Option<(usize, u32)>,
}

fn fresh_tune_chips(models: [u8; 2], clock: u32) -> Box<[sid; 2]> {
    let mut fresh = Box::new([sid::default(), sid::default()]);
    let mut fresh_dacs = Box::<[[sid_dac; 2]; 2]>::default();
    for (k, model) in models.into_iter().enumerate() {
        let is_8580 = model == 2;
        let (profile, sid_model) = if is_8580 {
            (SID_CHIP_8580_R5, SID_MODEL_8580)
        } else {
            (SID_CHIP_6581_R4AR, SID_MODEL_6581)
        };
        let mut t = Box::new(live_sid_target {
            chip: &mut fresh[k],
            clock_hz: clock,
            sample_rate: SAMPLE_RATE,
            dacs: fresh_dacs[k].as_mut_ptr(),
            ..Default::default()
        });
        let tp: *mut live_sid_target = &mut *t;
        unsafe {
            set_up_chip(tp, profile, sid_model);
            live_sid_sink(tp.cast(), LIVE_REG_GATE_RESET as u8, 0);
        }
    }
    fresh
}

pub fn render_host_psid(frames: usize, p: &PsidPlay) -> Vec<i16> {
    use sidou_tests::psid::{header, Player};
    let h = Box::new(header(p.file));
    let two = h.sid2 != 0;
    let clock = if h.clock == 2 {
        PSID_CLOCK_NTSC
    } else {
        PSID_CLOCK_PAL
    };
    let mut inst = HostInstrument::new();
    let mut b = HostChipB::with_keys_setup();
    let fresh = fresh_tune_chips([h.model1, h.model2], clock);
    let mut player = Player::with_chips(p.file, h, fresh);
    assert!(player.play(p.song), "the host's psid_ctl_play");
    let mut out = vec![0i16; 2 * frames];
    for (i, half) in out.chunks_mut(2 * HALF_FRAMES).enumerate() {
        let n = half.len() / 2;
        if p.b_rewrite_block == Some(i) {
            b.rewrite_keys_onto(&mut inst.live);
        }
        if p.b_use_block == Some(i) {
            b.wanted = true;
        }
        let fresh_a = p.fresh_block[0].is_some_and(|f| i >= f);
        let fresh_b = two && p.fresh_block[1].is_some_and(|f| i >= f);
        let tune_on = i >= p.start_block;
        if let Some((at, mask)) = p.mute_block {
            if i == at {
                assert!(player.mute(mask), "the host's psid_ctl_mute");
            }
        }
        let (mut sa, mut sb) = ([0i16; HALF_FRAMES], [0i16; HALF_FRAMES]);
        if tune_on {
            for _ in 0..TUNE_PASSES_PER_BLOCK {
                if !player.pass() {
                    break;
                }
            }
            if i == p.start_block {
                assert!(player.ctl.started != 0, "the host's tune has not started");
            }
            let (a, b, _) = player.block();
            sa = a;
            sb = b;
        } else if i + 1 == p.start_block {
            for _ in 0..TUNE_INIT_PASSES {
                player.pass();
                if player.ctl.started != 0 {
                    break;
                }
            }
        }
        let a: *mut sid = if fresh_a {
            &mut player.chips[0]
        } else {
            &mut *inst.chip
        };
        let chip_b: *mut sid = if fresh_b {
            &mut player.chips[1]
        } else {
            &mut *b.chip
        };
        unsafe {
            if i < p.psid_block {
                sid_render(a, inst.mono.as_mut_ptr(), n as u32);
            } else {
                inst.mono[..n].copy_from_slice(&sa[..n]);
            }
            let (b_in, b_rendered) = b.in_use(&*chip_b);
            if b_rendered {
                if two && i >= p.psid_block {
                    b.mono[..n].copy_from_slice(&sb[..n]);
                } else {
                    sid_render(chip_b, b.mono.as_mut_ptr(), n as u32);
                }
                b.mix_into(&mut inst.mono, n, b_in);
            }
            inst.finish_with_chip_b(a, chip_b, b_rendered, &b.over, half, POT, |_| {});
        }
    }
    assert_eq!(player.late(), 0, "the host's tune played a write late");
    out
}
