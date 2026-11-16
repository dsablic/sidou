// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;

use sidou_sys::*;

use crate::scene_rmr::{logo_pixel, Rig, W};

struct Intro {
    r: Box<scene_intro>,
    rig: Rig,
}

impl Intro {
    fn new() -> Intro {
        let mut r = Box::<scene_intro>::default();
        unsafe { scene_intro_init(&mut *r) };
        let mut intro = Intro { r, rig: Rig::new() };
        intro.draw();
        intro
    }

    fn draw(&mut self) {
        unsafe {
            scene_intro_draw(&mut *self.r, &mut *self.rig.sp);
            scene_intro_glow(&mut *self.r, &mut *self.rig.g, GRID_CELLS);
        }
    }

    fn frame(&mut self, hits: u32, cutoff: u32, sounding: bool) {
        let beat = scene_rmr_beat {
            hits: hits as u8,
            ..Default::default()
        };
        unsafe {
            if hits != 0 {
                scene_intro_beat(&mut *self.r, &beat);
            }
            scene_intro_frame(&mut *self.r, cutoff, i32::from(sounding));
        }
        self.rig.sp.ncand = 0;
        self.rig.sp.cand.fill(0);
        crate::drain(&mut self.rig.g);
        self.draw();
    }

    fn screen(&self) -> Vec<u8> {
        self.rig.screen()
    }

    fn stars(&self) -> Vec<(usize, usize, u8)> {
        let screen = self.screen();
        let logo = SCENE_INTRO_LOGO_Y0 as usize..SCENE_INTRO_LOGO_Y1 as usize;
        (0..W * W)
            .filter(|&i| screen[i] != GRID_BLACK as u8 && !logo.contains(&(i / W)))
            .map(|i| (i % W, i / W, screen[i]))
            .collect()
    }

    fn touched(&self) -> Vec<(usize, usize)> {
        (0..900)
            .filter(|&t| {
                self.rig.sp.cand[t / 8] >> (t % 8) & 1 != 0
                    || unsafe { grid_marked(&*self.rig.g, (t % 30) as u32, (t / 30) as u32) } != 0
            })
            .map(|t| (t % 30, t / 30))
            .collect()
    }
}

fn halo(p: u8) -> bool {
    p == GRID_PURPLE as u8 || p == GRID_BLUE as u8
}

fn logo_glow(screen: &[u8]) -> BTreeSet<u8> {
    let mut seen = BTreeSet::new();
    for y in SCENE_INTRO_LOGO_Y0 as usize..SCENE_INTRO_LOGO_Y1 as usize {
        for x in 0..W {
            let p = logo_pixel(x, y);
            if halo(p) {
                seen.insert(screen[y * W + x]);
            } else if p != GRID_BLACK as u8 {
                assert_eq!(screen[y * W + x], p, "the logo's chrome at ({x}, {y})");
            }
        }
    }
    seen
}

const SWEEP_FRAMES: u32 = SCENE_INTRO_LOGO_ROWS * SCENE_INTRO_GLOW_EVERY;

#[test]
fn the_intro_is_sidous_logo_between_starfields_drifting_left_at_three_depths() {
    let mut intro = Intro::new();
    let screen = intro.screen();
    let logo = (SCENE_INTRO_LOGO_Y0 as usize..SCENE_INTRO_LOGO_Y1 as usize)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| logo_pixel(x, y) != GRID_BLACK as u8)
        .inspect(|&(x, y)| assert_eq!(screen[y * W + x], logo_pixel(x, y), "({x}, {y})"))
        .count();
    assert!(logo > 4000, "{logo} pixels of the logo");
    let first = intro.stars();
    assert_eq!(first.len(), SCENE_INTRO_STARS as usize, "{first:?}");
    let depths: BTreeSet<u8> = first.iter().map(|s| s.2).collect();
    assert_eq!(depths.len(), SCENE_INTRO_STAR_DEPTHS as usize, "{depths:?}");
    let travel = |a: &[(usize, usize, u8)], b: &[(usize, usize, u8)]| -> Vec<(u8, usize)> {
        a.iter()
            .map(|s| {
                let t = b.iter().find(|t| t.1 == s.1).expect("the star's row");
                (s.2, (s.0 + W - t.0) % W)
            })
            .collect()
    };
    for _ in 0..16 {
        intro.frame(0, 0, false);
    }
    let calm = travel(&first, &intro.stars());
    let before = intro.stars();
    for _ in 0..16 {
        intro.frame(0, SCENE_INTRO_CUTOFF_TOP * 2, true);
    }
    let open = travel(&before, &intro.stars());
    println!("16 frames' travel: calm {calm:?}, the filter open {open:?}");
    for (c, o) in calm.iter().zip(&open) {
        assert!(
            c.1 > 0 && o.1 >= c.1 * (SCENE_INTRO_GEAR_TOP as usize - 1),
            "{c:?} {o:?}"
        );
    }
    for &(depth, d) in &calm {
        let deeper = calm.iter().filter(|s| s.1 < d).map(|s| s.0);
        assert!(
            deeper.clone().all(|c| c != depth),
            "a star of colour {depth}"
        );
    }
}

#[test]
fn an_intro_frame_redraws_only_its_stars_tiles_and_one_row_of_the_logos_glow() {
    let mut intro = Intro::new();
    for f in 0..3 * SWEEP_FRAMES {
        let hits = if f % 4 == 0 {
            SCENE_RMR_KICK << (f / 4 % 4)
        } else {
            0
        };
        let stars = intro.stars();
        intro.frame(hits, SCENE_INTRO_CUTOFF_TOP, true);
        let logo_rows = SCENE_INTRO_LOGO_Y0 as usize / 8..SCENE_INTRO_LOGO_Y1 as usize / 8;
        let touched = intro.touched();
        let (glow, starry): (Vec<&(usize, usize)>, Vec<_>) =
            touched.iter().partition(|t| logo_rows.contains(&t.1));
        let rows: BTreeSet<usize> = glow.iter().map(|t| t.1).collect();
        assert!(rows.len() <= 1, "frame {f}: logo rows {rows:?}");
        let now = intro.stars();
        for t in starry {
            let near = |s: &&(usize, usize, u8)| s.0 / 8 == t.0 && s.1 / 8 == t.1;
            assert!(
                stars.iter().any(|s| near(&s)) || now.iter().any(|s| near(&s)),
                "frame {f}: tile {t:?} has no star"
            );
        }
    }
}

#[test]
fn the_logos_glow_takes_the_colour_of_what_is_played_and_calms_in_silence() {
    let mut intro = Intro::new();
    let calm = logo_glow(&intro.screen());
    assert_eq!(calm, [GRID_PURPLE as u8, GRID_BLUE as u8].into());
    let mut glows = Vec::new();
    for hits in [
        SCENE_RMR_KICK,
        SCENE_RMR_SNARE,
        SCENE_RMR_HAT,
        SCENE_RMR_ARP,
    ] {
        intro.frame(hits, 0, true);
        for _ in 0..SWEEP_FRAMES / 2 {
            intro.frame(0, 0, true);
        }
        let half = logo_glow(&intro.screen());
        for _ in 0..SWEEP_FRAMES / 2 {
            intro.frame(0, 0, true);
        }
        let lit = logo_glow(&intro.screen());
        assert_eq!(lit.len(), 2, "{hits}: {lit:?}");
        assert!(
            half.len() > 2 && half.is_superset(&lit),
            "{hits}: the glow sweeps in, {half:?}"
        );
        assert!(lit.is_disjoint(&calm), "{hits}: {lit:?}");
        glows.push(lit);
    }
    for (i, a) in glows.iter().enumerate() {
        assert!(glows[i + 1..].iter().all(|b| b != a), "{glows:?}");
    }
    for _ in 0..SCENE_INTRO_CALM_FRAMES + SWEEP_FRAMES {
        intro.frame(0, 0, false);
    }
    assert_eq!(logo_glow(&intro.screen()), calm, "back to calm");
}
