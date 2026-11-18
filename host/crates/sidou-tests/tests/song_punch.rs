// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_sys::*;
use sidou_tests::song::*;
use sidou_tests::transport::*;

const BEAT_CALLS: u32 = 24;
const LEAD_CALLS: u32 = 40;
const MUTE_SETTLE_BLOCKS: usize = 32;
const FILTERED_SONG: &str = "dojo.sng";

struct Twin {
    _songs: [Song; 2],
    punched: Transport,
    plain: Transport,
}

impl Twin {
    fn playing(name: &str) -> Twin {
        let sng = gt2_ref::examples()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("{name} is among GoatTracker's examples"))
            .1;
        let mut songs = [Song::host_from(&sng), Song::host_from(&sng)];
        let mut punched = Transport::new(&mut songs[0]);
        let mut plain = Transport::new(&mut songs[1]);
        assert!(punched.play(0, 0) && plain.play(0, 0));
        let mut t = Twin {
            _songs: songs,
            punched,
            plain,
        };
        t.calls(LEAD_CALLS);
        t
    }

    fn step(&mut self) {
        for t in [&mut self.punched, &mut self.plain] {
            t.pass();
            t.block_silent();
        }
        assert_eq!(
            self.punched.calls(),
            self.plain.calls(),
            "the punched song keeps the plain one's clock"
        );
    }

    fn calls(&mut self, n: u32) {
        let until = self.plain.calls() + n;
        while self.plain.calls() < until {
            self.step();
        }
    }

    fn punch(&mut self, pads: u32) {
        assert!(self.punched.punch(pads, BEAT_CALLS));
        self.step();
    }

    fn until_handed_back(&mut self) {
        for _ in 0..5000 {
            if self.punched.sc.ff_on == 0 {
                return;
            }
            self.step();
        }
        panic!("the song never handed back");
    }

    fn assert_chips_equal(&self, what: &str) {
        let (a, b) = (&self.punched.chip, &self.plain.chip);
        assert_eq!(a.mute, b.mute, "{what}: the voice mutes");
        assert_eq!(
            (a.fc, a.res, a.routing, a.mode_vol),
            (b.fc, b.res, b.routing, b.mode_vol),
            "{what}: the filter and volume"
        );
        for v in 0..3 {
            assert_eq!(a.osc[v].freq, b.osc[v].freq, "{what}: voice {v}'s pitch");
            assert_eq!(a.osc[v].pw, b.osc[v].pw, "{what}: voice {v}'s pulse");
            assert_eq!(a.env[v].gate, b.env[v].gate, "{what}: voice {v}'s gate");
            assert_eq!(
                a.env[v].counter, b.env[v].counter,
                "{what}: voice {v}'s envelope"
            );
        }
    }

    fn assert_players_equal(&self, what: &str) {
        assert!(
            player_state(&self.punched.sc.player) == player_state(&self.plain.sc.player),
            "{what}: the punched player is not where the plain one is"
        );
    }
}

fn dive_q15(step: u32) -> u32 {
    let octave = f64::from(step.min(SONG_PUNCH_DIVE_STEPS)) / f64::from(SONG_PUNCH_DIVE_STEPS);
    (32768.0 * 2f64.powf(-octave)).round() as u32
}

#[test]
fn mute_silences_the_song_while_its_voices_run_on_and_release_gives_them_back_unchanged() {
    let mut t = Twin::playing(FILTERED_SONG);
    t.punch(SONG_PUNCH_MUTE);
    let mut out = [0i16; BLOCK];
    for block in 0..200 {
        t.punched.pass();
        t.plain.pass();
        t.punched.block(&mut out);
        t.plain.block_silent();
        let spread = out.iter().max().unwrap() - out.iter().min().unwrap();
        assert!(
            block < MUTE_SETTLE_BLOCKS || spread <= 1,
            "muted block {block} spans {spread}"
        );
    }
    assert_eq!(t.punched.chip.mute, 7, "all three voices are muted");
    t.punch(0);
    t.assert_chips_equal("after MUTE");
    t.assert_players_equal("after MUTE");
}

#[test]
fn filter_slam_closes_the_low_pass_at_full_resonance_and_release_restores_the_songs_filter() {
    let mut t = Twin::playing(FILTERED_SONG);
    t.punch(SONG_PUNCH_SLAM);
    for _ in 0..100 {
        t.calls(1);
        let c = &t.punched.chip;
        assert_eq!((c.fc, c.res, c.routing), (0, 15, 7), "the slammed filter");
        assert_eq!(c.mode_vol & 0x70, 0x10, "low pass only");
        assert_eq!(c.mode_vol & 0x0f, t.plain.chip.mode_vol & 0x0f, "the song's volume");
    }
    assert!(
        t.plain.chip.fc != 0 && t.plain.chip.routing != 0,
        "the song filters, so the restore is seen"
    );
    t.punch(0);
    t.calls(1);
    t.assert_chips_equal("after FILTER SLAM");
}

#[test]
fn pitch_dive_bends_every_voice_an_octave_down_over_half_a_second_and_release_snaps_back() {
    let mut t = Twin::playing(FILTERED_SONG);
    t.punch(SONG_PUNCH_DIVE);
    let mut deepest = 0;
    for held in 1..60u32 {
        t.calls(1);
        let sc = &t.punched.sc;
        let step = (sc.samples - BLOCK as u32 - sc.dive_at)
            / (SONG_PUNCH_DIVE_SAMPLES / SONG_PUNCH_DIVE_STEPS);
        for v in 0..3 {
            let want = u32::from(t.plain.chip.osc[v].freq) * dive_q15(step) / 32768;
            assert_eq!(
                u32::from(t.punched.chip.osc[v].freq),
                want,
                "voice {v} after {held} calls (step {step})"
            );
        }
        deepest = deepest.max(step.min(SONG_PUNCH_DIVE_STEPS));
    }
    assert_eq!(dive_q15(deepest), 16384, "a whole octave down");
    t.punch(0);
    t.calls(1);
    t.assert_chips_equal("after PITCH DIVE");
}

#[test]
fn stutter_repeats_a_beat_and_release_resumes_where_the_song_would_have_been() {
    let mut t = Twin::playing(FILTERED_SONG);
    t.punch(SONG_PUNCH_STUTTER);
    let mut held = Vec::new();
    for _ in 0..3 * BEAT_CALLS {
        t.calls(1);
        held.push(player_state(&t.punched.sc.player));
    }
    for k in 0..2 * BEAT_CALLS as usize {
        assert!(
            held[k] == held[k + BEAT_CALLS as usize],
            "call {k} of the stutter differs a beat later"
        );
    }
    t.punch(0);
    t.until_handed_back();
    assert_eq!(t.punched.sc.handovers, 1, "the song came back by one hand-over");
    t.calls(1);
    t.assert_players_equal("after STUTTER");
    t.calls(BEAT_CALLS);
    t.assert_players_equal("a beat after STUTTER");
    t.assert_chips_equal("a beat after STUTTER");
}

#[test]
fn every_pad_at_once_then_none_leaves_the_song_as_if_never_punched() {
    let mut t = Twin::playing(FILTERED_SONG);
    t.punch(SONG_PUNCH_PADS);
    t.calls(2 * BEAT_CALLS + 5);
    assert_eq!(t.punched.chip.mute, 7);
    assert_eq!(t.punched.chip.fc, 0);
    t.punch(SONG_PUNCH_SLAM);
    assert_eq!(t.punched.chip.mute, 0, "MUTE let go while SLAM is held");
    t.calls(1);
    assert_eq!(t.punched.chip.fc, 0, "SLAM still held");
    t.punch(0);
    t.until_handed_back();
    t.calls(BEAT_CALLS);
    t.assert_players_equal("after every pad");
    t.assert_chips_equal("after every pad");
}

#[test]
fn the_pads_do_nothing_while_the_song_is_stopped_and_stop_lets_them_go() {
    let mut t = Twin::playing(FILTERED_SONG);
    t.punch(SONG_PUNCH_PADS);
    t.calls(3);
    assert!(t.punched.stop());
    t.punched.pass();
    t.punched.block_silent();
    t.punched.pass();
    assert_eq!(t.punched.chip.mute, 0, "stop lets MUTE go");
    assert_eq!(t.punched.sc.punch, 0, "stop lets every pad go");
    assert_eq!(t.punched.sc.ff_on, 0, "stop ends the stutter");
    assert!(t.punched.punch(SONG_PUNCH_PADS, BEAT_CALLS));
    t.punched.pass();
    t.punched.block_silent();
    assert_eq!(t.punched.sc.punch, 0, "no pad while stopped");
    assert_eq!(t.punched.chip.mute, 0);
}
