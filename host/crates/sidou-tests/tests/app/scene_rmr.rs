// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use sidou_sys::*;
use sidou_tests::font::repo_root;
use sidou_tests::scene_art;

use crate::{shipped_art, STATUS_BAR_H};

pub(crate) const W: usize = 240;
const POSE_NAMES: [&str; 9] = [
    "guard", "guard2", "raise", "chop", "block", "spinA", "spinB", "hit", "reach",
];

fn golden_dir() -> PathBuf {
    repo_root().join("tests/golden/scene")
}

struct TraceFrame {
    frame: u32,
    steps: Vec<scene_rmr_beat>,
    cutoff: u32,
    state: String,
}

fn beat_of(step: &str) -> scene_rmr_beat {
    let f: Vec<&str> = step.split('.').collect();
    let flags = f[2];
    let mut hits = 0;
    for (c, h) in [
        ('k', SCENE_RMR_KICK),
        ('s', SCENE_RMR_SNARE),
        ('h', SCENE_RMR_HAT),
        ('a', SCENE_RMR_ARP),
    ] {
        if flags.contains(c) {
            hits |= h;
        }
    }
    let bass = match f[3] {
        "-" => SCENE_RMR_BASS_NONE,
        n if n.parse::<u32>().unwrap() >= SCENE_SONG_BASS_SPLIT => SCENE_RMR_BASS_HIGH,
        _ => SCENE_RMR_BASS_LOW,
    };
    scene_rmr_beat {
        row: 1,
        s: f[0].parse().unwrap(),
        bar: f[1].parse().unwrap(),
        rpb: 4,
        hits: hits as u8,
        bass: bass as u8,
    }
}

fn trace() -> Vec<TraceFrame> {
    let text = std::fs::read_to_string(golden_dir().join("rmr-trace.tsv")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let w: Vec<&str> = l.split(' ').collect();
            let steps = if w[1] == "-" {
                Vec::new()
            } else {
                w[1].split(',').map(beat_of).collect()
            };
            TraceFrame {
                frame: w[0].parse().unwrap(),
                steps,
                cutoff: w[2].parse().unwrap(),
                state: w[3..].join(" "),
            }
        })
        .collect()
}

fn state_of(r: &scene_rmr) -> String {
    let pose = |k: usize| POSE_NAMES[usize::from(r.pose[k])];
    let next = if r.next == 0 {
        "-".to_string()
    } else {
        POSE_NAMES[usize::from(r.next - 1)].to_string()
    };
    let (mut gx, mut gy, mut k) = (0i32, 0i32, 0u32);
    let sword = if unsafe { scene_rmr_sword_at(r, &mut gx, &mut gy, &mut k) } != 0 {
        let t = unsafe { scene_rmr_flights[r.unarmed as usize].frames };
        format!("{}/{t}/{gx}/{gy}/{k}", r.sword_n)
    } else {
        "-".to_string()
    };
    let sky: String = r.sky.iter().map(|p| p.to_string()).collect();
    format!(
        "{} {} {},{} {},{} {},{} {} {} {} {} {} {} {next} {sword} {sky}",
        r.x,
        r.lead,
        pose(0),
        pose(1),
        r.t[0],
        r.t[1],
        r.hp[0],
        r.hp[1],
        r.unarmed,
        r.spark,
        r.spin,
        r.spin_who,
        r.shake,
        r.flare
    )
}

fn played(upto: u32) -> Box<scene_rmr> {
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    for f in trace().iter().take_while(|f| f.frame <= upto) {
        for b in &f.steps {
            unsafe { scene_rmr_step(&mut *r, b) };
        }
        unsafe { scene_rmr_frame(&mut *r, f.cutoff, 1) };
    }
    r
}

#[test]
fn the_vignette_plays_the_script_frame_for_frame() {
    let t = trace();
    assert!(t.len() >= 700, "{} frames", t.len());
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    let (mut disarms, mut stuck, mut reaches, mut parries) = (0, 0, 0, 0);
    let mut was = state_of(&r);
    for f in &t {
        for b in &f.steps {
            unsafe { scene_rmr_step(&mut *r, b) };
        }
        unsafe { scene_rmr_frame(&mut *r, f.cutoff, 1) };
        let now = state_of(&r);
        assert_eq!(
            now, f.state,
            "frame {}: the device (left) and the script (right); the frame before: {was}",
            f.frame
        );
        disarms += usize::from(r.sword != 0 && r.sword_n == 1);
        stuck += usize::from(r.sword != 0 && now.contains("/8 "));
        reaches +=
            usize::from(r.unarmed >= 0 && u32::from(r.pose[r.unarmed as usize]) == SCENE_RMR_REACH);
        parries += usize::from(r.spark == 2);
        was = now;
    }
    println!(
        "{disarms} disarms, {stuck} frames stuck, {reaches} frames reaching, {parries} parries"
    );
    assert_eq!(disarms, 3);
    assert!(stuck > 10 && reaches == 3 * 7 && parries > 10);
}

#[test]
fn the_sky_takes_the_cutoffs_palette() {
    let of = |c: u32| unsafe { scene_rmr_sky_of(c) };
    assert_eq!(
        (of(0), of(736), of(737), of(1391), of(1392), of(2047)),
        (0, 0, 1, 1, 2, 2)
    );
    let mut r = Box::<scene_rmr>::default();
    unsafe { scene_rmr_init(&mut *r) };
    for f in 0..15 {
        unsafe { scene_rmr_frame(&mut *r, 2047, 1) };
        assert_eq!(u32::from(r.sky_row), 2 + f, "a row a frame, from the top");
    }
    unsafe { scene_rmr_frame(&mut *r, 2047, 1) };
    assert_eq!(r.sky_row, 0xff, "wiped");
    assert!(r.sky.iter().all(|&p| p == 2));
}

pub(crate) struct Rig {
    pub(crate) g: Box<grid>,
    pub(crate) sp: Box<scene_px>,
}

impl Rig {
    pub(crate) fn new() -> Rig {
        let mut g = Box::<grid>::default();
        let mut sp = Box::<scene_px>::default();
        unsafe {
            grid_init(&mut *g, b' ', 0);
            scene_px_init(&mut *sp, &mut *g);
        }
        Rig { g, sp }
    }

    pub(crate) fn screen(&self) -> Vec<u8> {
        let mut px = vec![0u8; W * W];
        for t in 0..900usize {
            let mut tile = [0u8; 64];
            unsafe {
                scene_px_tile(
                    &*self.sp,
                    (t % 30) as u32,
                    (t / 30) as u32,
                    tile.as_mut_ptr(),
                )
            };
            for y in 0..8 {
                let at = ((t / 30) * 8 + y) * W + (t % 30) * 8;
                px[at..at + 8].copy_from_slice(&tile[y * 8..y * 8 + 8]);
            }
        }
        px
    }
}

fn left_out_of_the_script_goldens(x: usize, y: usize) -> bool {
    let f = unsafe { scene_rmr_flames };
    y < STATUS_BAR_H
        || f.iter().any(|b| {
            let (bx, by, bw, bh) = (
                usize::from(b[0]),
                usize::from(b[1]),
                usize::from(b[2]),
                usize::from(b[3]),
            );
            x >= bx && x < bx + bw && y + 14 >= by && y < by + bh
        })
}

#[test]
fn the_layers_draw_the_scripts_golden_frames() {
    let names = ["guard", "parry", "spin", "flight", "stuck", "reach", "wipe"];
    for name in names {
        let text = std::fs::read_to_string(golden_dir().join(format!("rmr-{name}.txt"))).unwrap();
        let frame: u32 = text
            .lines()
            .next()
            .and_then(|l| l.split("frame ").nth(1))
            .and_then(|l| l.split(' ').next())
            .unwrap()
            .parse()
            .unwrap();
        let png = scene_art::parse_png(
            &std::fs::read(golden_dir().join(format!("rmr-{name}.png"))).unwrap(),
        )
        .unwrap();
        let mut r = played(frame);
        let mut rig = Rig::new();
        unsafe { scene_rmr_draw(&mut *r, shipped_art(), &mut *rig.sp) };
        let screen = rig.screen();
        let mut off = Vec::new();
        for y in 0..W {
            for x in 0..W {
                if !left_out_of_the_script_goldens(x, y) && screen[y * W + x] != png.px[y * W + x] {
                    off.push((x, y));
                }
            }
        }
        let spin_arc_off_by_a_pixel = if name == "spin" {
            2 * SCENE_RMR_ARC_MARKS as usize
        } else {
            0
        };
        println!("{name} (frame {frame}): {} pixels off", off.len());
        assert!(
            off.len() <= spin_arc_off_by_a_pixel,
            "{name}: {} pixels differ from the script's, the first {:?}",
            off.len(),
            &off[..off.len().min(8)]
        );
    }
}

fn view_tile(ui: &crate::views::Ui, col: usize, row: usize) -> [u8; 64] {
    let mut tile = [0u8; 64];
    let sp = unsafe { &ui.v.scene.u.rite.px };
    unsafe { scene_px_tile(sp, col as u32, row as u32, tile.as_mut_ptr()) };
    tile
}

#[test]
fn red_moon_rite_is_scenes_sixth_piece_and_paints_under_the_screens_overlays() {
    use crate::views::Ui;
    let mut ui = Ui::new();
    crate::scene::open_scene(&mut ui);
    ui.turn(PANEL_ENC_SELECT, -1);
    assert_eq!(u32::from(ui.v.scene.which), SCENE_RMR);
    ui.frame();
    assert_eq!(ui.toast().as_deref(), Some("RED MOON RITE"));
    ui.settle();
    crate::drain(&mut ui.g);
    let sp: *const scene_px = unsafe { &ui.v.scene.u.rite.px };
    assert_eq!(ui.g.region_ctx.cast_const(), sp.cast());
    let (c, r) = (SCREEN_TOAST_COL as usize, SCREEN_TOAST_ROW as usize);
    let bg = ui.g.attr[r * 30 + c] >> 4;
    assert_ne!(ui.g.ch[r * 30 + c], b' ');
    ui.frame();
    assert!(
        view_tile(&ui, c, r).iter().all(|&p| p == bg),
        "the toast's background"
    );
    ui.frames_sent(40);
    assert!(ui.toast().is_none());
    assert!(
        view_tile(&ui, c, r).iter().any(|&p| p != bg),
        "the floor under the toast's row once it is gone"
    );
    let _ml = ui.attach_learn();
    ui.learn_on(PANEL_ENC_K1);
    ui.frame();
    ui.frame();
    assert_eq!(ui.top(), "LEARN");
    assert_eq!(
        ui.g.region_ctx.cast_const(),
        sp.cast(),
        "the region stays under the frame"
    );
    let (c, r) = (LEARN_VIEW_COL as usize + 1, LEARN_VIEW_ROW as usize + 1);
    let bg = ui.g.attr[r * 30 + c] >> 4;
    assert!(
        view_tile(&ui, c, r).iter().all(|&p| p == bg),
        "the frame's background"
    );
    ui.press(PANEL_BTN_HOME);
    ui.frame();
    assert_eq!(ui.top(), "SCENE");
    ui.frame();
    assert!(
        view_tile(&ui, c, r).iter().any(|&p| p != bg),
        "the scene back under it"
    );
    ui.turn(PANEL_ENC_SELECT, 1);
    ui.frame();
    assert!(ui.g.region_painter.is_none());
    assert_eq!(u32::from(crate::petscii(&ui.v.scene).piece), SCENE_FIRE);
}

pub(crate) fn logo_pixel(x: usize, y: usize) -> u8 {
    let mut px = [0u8; 1];
    unsafe {
        grid_bitmap_row(
            &logo_metal_chrome_bitmap,
            x as u32,
            y as u32,
            1,
            px.as_mut_ptr(),
        )
    };
    px[0]
}

fn open_rmr(st: *const store) -> crate::views::Ui {
    let mut ui = crate::views::Ui::new();
    crate::scene::open_scene(&mut ui);
    ui.v.scene.st = st;
    ui.turn(PANEL_ENC_SELECT, -1);
    ui.frame();
    ui
}

fn shows_art(ui: &crate::views::Ui) -> bool {
    unsafe { ui.v.scene.u.rite.has_art != 0 }
}

fn shown_screen(ui: &crate::views::Ui) -> Vec<u8> {
    let mut px = vec![0u8; W * W];
    for t in 0..900usize {
        let tile = view_tile(ui, t % 30, t / 30);
        for y in 0..8 {
            let at = ((t / 30) * 8 + y) * W + (t % 30) * 8;
            px[at..at + 8].copy_from_slice(&tile[y * 8..y * 8 + 8]);
        }
    }
    px
}

fn shows_the_logo(ui: &crate::views::Ui) -> bool {
    let screen = shown_screen(ui);
    (SCENE_INTRO_LOGO_Y0 as usize..SCENE_INTRO_LOGO_Y1 as usize)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| logo_pixel(x, y) != GRID_BLACK as u8)
        .all(|(x, y)| screen[y * W + x] == logo_pixel(x, y))
}

struct ToastBox {
    text: Vec<u8>,
    rest: Vec<(usize, usize)>,
}

fn toast_box(ui: &crate::views::Ui, g: &grid) -> ToastBox {
    let s = &ui.v.s;
    let lcd = crate::pixels(g);
    let words = ui.toast().unwrap_or_default();
    let brk = if s.toast_h == 4 {
        s.toast_brk as usize
    } else {
        words.len()
    };
    let lines = [brk, words.len().saturating_sub(brk + 1)];
    let (c0, r0) = (SCREEN_TOAST_COL as usize, SCREEN_TOAST_ROW as usize);
    let first = if s.toast_h == 4 { r0 - 2 } else { r0 };
    let mut b = ToastBox {
        text: Vec::new(),
        rest: Vec::new(),
    };
    for y in first * 8..(first + s.toast_h as usize) * 8 {
        let line = lines[(y / 8 - first) / 2];
        for x in c0 * 8..(c0 + s.toast_w as usize) * 8 {
            if (x / 8 - c0) < 2 * line {
                b.text.push(lcd[y * W + x]);
            } else {
                b.rest.push((x, y));
            }
        }
    }
    b
}

#[test]
fn red_moon_rites_toasts_show_their_big_text_over_the_scene_in_every_theme() {
    for id in 0..THEME_COUNT {
        let th = unsafe { &*theme_get(id) };
        assert_ne!(th.toast_fg, th.toast_bg, "theme {id}");
        let fresh = crate::store::fresh(0xa27, STORE_WINDOW_MAX);
        let shown = [
            (&*fresh.st as *const store, hint()),
            (crate::art_store(), String::from("RED MOON RITE")),
        ];
        for (st, text) in shown {
            let mut ui = crate::views::Ui::new();
            unsafe { live_set_theme(&mut *ui.rig.live, id) };
            crate::scene::open_scene(&mut ui);
            ui.v.scene.st = st;
            for _ in 1..SCENE_RMR {
                ui.turn(PANEL_ENC_SELECT, 1);
                ui.frames_sent(40);
            }
            ui.turn(PANEL_ENC_SELECT, 1);
            ui.frames_sent(20);
            assert_eq!(ui.toast().as_deref(), Some(text.as_str()));
            assert!(ui.g.region_painter.is_some());
            let mut plain = ui.g.clone();
            plain.region_painter = None;
            let b = toast_box(&ui, &ui.g);
            assert!(b.text.contains(&th.toast_fg), "theme {id}, {text}: no text");
            assert!(
                b.text.iter().all(|&p| p == th.toast_fg || p == th.toast_bg),
                "theme {id}, {text}: the toast's text holds other colours"
            );
            assert!(
                b.text == toast_box(&ui, &plain).text,
                "theme {id}, {text}: the toast over the scene differs from the toast elsewhere"
            );
            let stale: std::collections::BTreeSet<(usize, usize, u8, u8)> = b
                .rest
                .iter()
                .map(|&(x, y)| (x / 8, y / 8))
                .map(|(c, r)| (c, r, ui.g.ch[r * 30 + c], ui.g.attr[r * 30 + c]))
                .filter(|&(_, _, ch, attr)| ch != b' ' || attr != 0)
                .collect();
            assert!(
                stale.is_empty(),
                "theme {id}, {text}: cells beside a short line hold {stale:?}"
            );
        }
    }
}

fn hint() -> String {
    String::from_utf8(SCENE_VIEW_ART_HINT.split_last().unwrap().1.to_vec()).unwrap()
}

fn resealed(mut pack: Vec<u8>) -> Vec<u8> {
    let n = pack.len() - 4;
    let crc = scene_art::crc32(&pack[..n]);
    pack[n..].copy_from_slice(&crc.to_le_bytes());
    pack
}

#[test]
fn a_fresh_store_plays_the_intro_and_a_sent_pack_replaces_it_at_once() {
    let mut rig = crate::store::fresh(0xa27, STORE_WINDOW_MAX);
    let mut ui = open_rmr(&*rig.st);
    assert_eq!(ui.toast().as_deref(), Some(hint().as_str()));
    ui.frames_sent(40);
    assert!(!shows_art(&ui));
    assert!(shows_the_logo(&ui), "the intro's logo");
    crate::sid_io::put(
        &mut rig,
        sidou_sysex::art::NAME,
        STORE_FILE_ART,
        crate::shipped_pack().bytes(),
    );
    ui.frames_sent(SCENE_VIEW_ART_LOOK_FRAMES + 2);
    assert!(shows_art(&ui), "the sent pack");
    assert!(!shows_the_logo(&ui), "RED MOON RITE's art over the logo");
    let c = std::ffi::CString::new(sidou_sysex::art::NAME).unwrap();
    assert_eq!(
        rig.api(|s| unsafe { store_file_delete(s, c.as_ptr()) }),
        STORE_OK
    );
    rig.settle();
    ui.frames_sent(2);
    assert!(!shows_art(&ui), "the pack deleted: the intro again");
    assert!(shows_the_logo(&ui));
}

#[test]
fn a_damaged_pack_falls_back_to_the_intro() {
    let good = crate::shipped_pack().bytes().to_vec();
    let mut flipped = good.clone();
    let data = good.len() / 2;
    flipped[data] ^= 0x10;
    let mut slot = good.clone();
    slot[ART_PACK_HEAD as usize + 1] = 65;
    let mut frame = good.clone();
    frame[ART_PACK_TABLES as usize + 2] = 200;
    for (what, bytes) in [
        ("a byte changed under its CRC", flipped),
        ("a slot's width", resealed(slot)),
        ("a frame wider than its slot", resealed(frame)),
    ] {
        let rig = crate::store_with_art(&bytes);
        let mut ui = open_rmr(&*rig.st);
        assert_eq!(ui.toast().as_deref(), Some(hint().as_str()), "{what}");
        ui.frames_sent(40);
        assert!(!shows_art(&ui), "{what}");
        assert!(shows_the_logo(&ui), "{what}");
    }
}

const HIGH: u8 = SCENE_RMR_HIGH_NOTE as u8 + 12;
const LOW: u8 = SCENE_RMR_HIGH_NOTE as u8 - 12;
const MEDIUM: u8 = SCENE_RMR_SOFT as u8;
const HARD: u8 = SCENE_RMR_HARD as u8;
const SKY_CUTOFF: u32 = 1200;

fn on(chip: u8, pitch: u8, velocity: u8) -> scene_rmr_note {
    scene_rmr_note {
        chip,
        pitch,
        velocity,
    }
}

struct Duel {
    r: Box<scene_rmr>,
    poses: Vec<[u8; 2]>,
}

impl Duel {
    fn new() -> Duel {
        let mut r = Box::<scene_rmr>::default();
        unsafe { scene_rmr_init(&mut *r) };
        Duel {
            r,
            poses: Vec::new(),
        }
    }

    fn frame(&mut self, notes: &[scene_rmr_note], held: bool) {
        let sounding = held || !notes.is_empty();
        unsafe {
            scene_rmr_notes(&mut *self.r, notes.as_ptr(), notes.len() as u32, i32::from(sounding));
            scene_rmr_frame(&mut *self.r, SKY_CUTOFF, i32::from(sounding));
        }
        self.poses.push(self.r.pose);
    }

    fn rest(&mut self, frames: u32, held: bool) {
        for _ in 0..frames {
            self.frame(&[], held);
        }
    }

    fn saw(&self, k: usize, pose: u32) -> bool {
        self.poses.iter().any(|p| u32::from(p[k]) == pose)
    }
}

#[test]
fn a_note_on_chip_a_is_the_left_fighters_attack_and_on_chip_b_the_rights() {
    for chip in 0..2u8 {
        let mut d = Duel::new();
        d.frame(&[on(chip, HIGH, MEDIUM)], true);
        d.rest(SCENE_RMR_PARRY_FRAMES + 8, false);
        let k = usize::from(chip);
        assert_eq!(u32::from(d.poses[0][k]), SCENE_RMR_RAISE, "chip {chip}");
        assert!(d.saw(k, SCENE_RMR_CHOP), "chip {chip}: no chop");
        assert!(!d.saw(1 - k, SCENE_RMR_RAISE) && !d.saw(1 - k, SCENE_RMR_BLOCK));
        assert_eq!(u32::from(d.r.lead), u32::from(chip), "the attacker presses on");
    }
}

#[test]
fn high_notes_chop_overhead_and_low_notes_sweep_low() {
    let first_poses = |pitch: u8| {
        let mut d = Duel::new();
        d.frame(&[on(0, pitch, MEDIUM)], true);
        d.rest(10, true);
        let mut seen: Vec<u32> = d.poses.iter().map(|p| u32::from(p[0])).collect();
        seen.dedup();
        seen
    };
    let guard = SCENE_RMR_GUARD;
    assert_eq!(first_poses(HIGH)[..3], [SCENE_RMR_RAISE, SCENE_RMR_CHOP, guard]);
    assert_eq!(first_poses(LOW)[..3], [SCENE_RMR_SPIN_A, SCENE_RMR_SPIN_B, guard]);
}

#[test]
fn a_second_note_soon_after_is_the_others_parry_its_spark_and_shake_by_velocity() {
    for (velocity, spark, shake) in [(1, 1, 0), (MEDIUM, 2, 0), (HARD, 3, 3)] {
        let mut d = Duel::new();
        d.frame(&[on(0, HIGH, MEDIUM)], true);
        d.rest(SCENE_RMR_PARRY_FRAMES / 2, true);
        unsafe {
            scene_rmr_notes(&mut *d.r, &on(0, HIGH, velocity), 1, 1);
        }
        assert_eq!(u32::from(d.r.pose[1]), SCENE_RMR_BLOCK, "velocity {velocity}");
        assert_eq!(u32::from(d.r.spark), spark, "velocity {velocity}: the spark");
        assert_eq!(u32::from(d.r.shake), shake, "velocity {velocity}: the shake");
        assert_eq!(u32::from(d.r.hp[0]), SCENE_RMR_HP - 1, "the parried attacker tires");
    }
    let mut late = Duel::new();
    late.frame(&[on(0, HIGH, MEDIUM)], true);
    late.rest(SCENE_RMR_PARRY_FRAMES + 1, true);
    late.frame(&[on(1, HIGH, MEDIUM)], true);
    assert!(!late.saw(0, SCENE_RMR_BLOCK), "a late note parries");
    assert_eq!(u32::from(late.r.pose[1]), SCENE_RMR_RAISE, "a late note is an attack");
}

#[test]
fn a_chord_flares_the_torches() {
    let mut d = Duel::new();
    d.frame(&[on(0, HIGH, MEDIUM), on(0, HIGH + 4, MEDIUM)], true);
    assert_eq!(d.r.flame_big, 0, "two notes are not a chord");
    d.rest(SCENE_RMR_RUN_GAP + 1, false);
    d.frame(
        &[on(0, HIGH, MEDIUM), on(0, HIGH + 4, MEDIUM), on(0, HIGH + 7, MEDIUM)],
        true,
    );
    assert_eq!(d.r.flame_big, 1);
}

#[test]
fn a_held_note_sets_the_pair_circling() {
    let mut d = Duel::new();
    d.frame(&[on(0, HIGH, MEDIUM)], true);
    d.rest(SCENE_RMR_HOLD_FRAMES, true);
    let rest_x = d.r.x;
    let stance = d.r.stance;
    let mut xs = Vec::new();
    for _ in 0..4 * SCENE_RMR_STANCE_FRAMES {
        d.frame(&[], true);
        xs.push(d.r.x);
    }
    let (lo, hi) = (*xs.iter().min().unwrap(), *xs.iter().max().unwrap());
    assert!(hi - lo >= SCENE_RMR_SWAY as i16, "the pair sways {lo}..{hi}");
    assert!(xs.iter().any(|&x| x != rest_x));
    assert!(d.r.stance != stance || d.poses.iter().any(|p| u32::from(p[0]) == SCENE_RMR_GUARD2));
    let mut still = Duel::new();
    still.frame(&[on(0, HIGH, MEDIUM)], true);
    still.rest(SCENE_RMR_HOLD_FRAMES + 4 * SCENE_RMR_STANCE_FRAMES, false);
    assert_eq!(still.r.sway, 0, "let go, the pair stands");
}

fn run(d: &mut Duel, notes: u32, gap: u32) {
    for i in 0..notes {
        let pitch = if i % 2 == 0 { HIGH } else { LOW };
        d.frame(&[on(0, pitch, MEDIUM)], true);
        d.rest(gap - 1, true);
    }
}

#[test]
fn a_fast_run_is_a_flurry_of_quick_strikes_each_met_by_a_block() {
    let mut d = Duel::new();
    run(&mut d, SCENE_RMR_FLURRY - 1, SCENE_RMR_RUN_GAP);
    let before = d.poses.len();
    d.frame(&[on(0, HIGH, MEDIUM)], true);
    assert_eq!(u32::from(d.r.pose[0]), SCENE_RMR_CHOP, "no wind-up in a flurry");
    assert_eq!(u32::from(d.r.pose[1]), SCENE_RMR_BLOCK);
    assert!(d.r.spark > 0);
    d.rest(SCENE_RMR_RUN_GAP + 1, false);
    d.frame(&[on(0, HIGH, MEDIUM)], true);
    assert_eq!(
        u32::from(d.r.pose[0]),
        SCENE_RMR_RAISE,
        "a pause ends the run: {:?}",
        &d.poses[before..]
    );
}

#[test]
fn a_long_run_is_the_spinning_attack_and_the_disarm_and_the_sword_is_taken_back() {
    let mut d = Duel::new();
    run(&mut d, SCENE_RMR_RUN_LONG, 3);
    assert!(d.r.spin > 0 || d.saw(0, SCENE_RMR_SPIN_A), "no spin");
    d.rest(5 * SCENE_RMR_SPIN_FRAMES, true);
    assert_eq!(d.r.unarmed, 1, "the right fighter is disarmed");
    assert_eq!(d.r.sword, 1);
    assert!(d.saw(1, SCENE_RMR_HIT));
    let flight = unsafe { scene_rmr_flights[1].frames };
    d.rest(u32::from(flight) + SCENE_RMR_STUCK_FRAMES + 10, false);
    assert!(d.saw(1, SCENE_RMR_REACH), "the sword is never taken back");
    assert_eq!((d.r.unarmed, d.r.sword), (-1, 0));
}

#[test]
fn while_one_is_unarmed_nobody_attacks_or_parries() {
    let mut d = Duel::new();
    run(&mut d, SCENE_RMR_RUN_LONG, 3);
    d.rest(5 * SCENE_RMR_SPIN_FRAMES, true);
    assert_eq!(d.r.unarmed, 1);
    let from = d.poses.len();
    d.rest(SCENE_RMR_RUN_GAP + 1, false);
    for _ in 0..3 {
        d.frame(&[on(0, HIGH, HARD), on(1, LOW, HARD)], true);
        d.rest(3, true);
    }
    let attacked = d.poses[from..]
        .iter()
        .flatten()
        .any(|&p| [SCENE_RMR_RAISE, SCENE_RMR_CHOP, SCENE_RMR_BLOCK].contains(&u32::from(p)));
    assert!(!attacked);
}

#[test]
fn the_same_notes_play_the_same_duel() {
    let play = || {
        let mut d = Duel::new();
        let mut rng = 0x51d0_u32;
        let mut states = Vec::new();
        for _ in 0..600 {
            let r = unsafe { scene_random(&mut rng) };
            let notes: Vec<scene_rmr_note> = (0..r % 7 / 2)
                .map(|i| on((r >> 8) as u8 & 1, 36 + (r >> (10 + i)) as u8 % 48, (r >> 16) as u8 & 127))
                .collect();
            d.frame(&notes, r & 0x300 != 0);
            states.push(state_of(&d.r));
        }
        states
    };
    let first = play();
    assert_eq!(first, play());
    let mut kinds: Vec<&str> = first.iter().map(|s| s.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert!(kinds.len() > 100, "{} different states", kinds.len());
}
