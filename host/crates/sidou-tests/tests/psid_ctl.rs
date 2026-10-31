// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use sidou_sys::*;
use sidou_tests::psid::*;
use sidou_tests::psid_fixtures::*;

const PASSES: usize = 8;

const LONGEST_INSTRUCTION_OVERSHOOT: u32 = 7;
const AHEAD_BOUND: u32 = PSID_AHEAD_CYCLES + PSID_SLICE_CYCLES + LONGEST_INSTRUCTION_OVERSHOOT;
const STA_ABS_CLC_ADC_IMM_JMP_CYCLES: u32 = 11;

fn blocks_for(seconds: u32) -> usize {
    (seconds * SAMPLE_RATE) as usize / BLOCK
}

fn since_the_audio_started<'a>(p: &Player, out: &'a [i16]) -> &'a [i16] {
    &out[(out.len() / BLOCK - p.blocks.len()) * BLOCK..]
}

fn first_difference<T: PartialEq>(got: &[T], want: &[T]) -> Option<usize> {
    got.iter().zip(want).position(|(x, y)| x != y)
}

fn assert_exact_against_a_reference_render(
    p: &Player,
    file: &[u8],
    a: &[i16],
    b: &[i16],
    what: &str,
) {
    let mut chips = chips_for(&header(file));
    let (ra, rb) = reference(&mut chips, &p.as_applied(), p.blocks.len());
    assert_eq!(
        first_difference(since_the_audio_started(p, a), &ra),
        None,
        "{what}: chip A differs"
    );
    if header(file).sid2 != 0 {
        assert_eq!(
            first_difference(since_the_audio_started(p, b), &rb),
            None,
            "{what}: chip B differs"
        );
    }
}

fn corpus() -> Vec<(String, Vec<u8>, u32)> {
    let mut jobs = Vec::new();
    for (name, sng) in songs() {
        for sid in [0xD400, 0xD420] {
            let file = packed(&sng, sid);
            for song in 1..=u32::from(header(&file).songs) {
                jobs.push((
                    format!("{name} at ${sid:04X}, song {song}"),
                    file.clone(),
                    song,
                ));
            }
        }
    }
    jobs
}

fn on_every_core<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<T>>> = Mutex::new((0..n).map(|_| None).collect());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    break;
                }
                let r = f(i);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner()
        .unwrap()
        .into_iter()
        .map(|r| r.unwrap())
        .collect()
}

#[test]
fn the_placement_is_exact_over_the_corpus() {
    let seconds = if cfg!(debug_assertions) { 3 } else { 20 };
    let jobs = corpus();
    let results = on_every_core(jobs.len(), |i| {
        let (name, file, song) = &jobs[i];
        let mut p = Player::new(file);
        assert!(p.play(*song), "{name}");
        let (a, b) = p.run(blocks_for(seconds), PASSES);
        assert_eq!(p.late(), 0, "{name}: late writes");
        assert_eq!(p.ctl.dropped, 0, "{name}");
        assert!(p.ahead_max <= AHEAD_BOUND, "{name}: {} ahead", p.ahead_max);
        assert!(p.ctl.splits_max <= PSID_SPLITS, "{name}");
        let s = stream(file, *song, p.log.last().map_or(0, |w| w.cycle + 1));
        assert_same_writes(&p.log, &s, name);
        assert_eq!(p.as_applied(), p.log, "{name}");
        assert_exact_against_a_reference_render(&p, file, &a, &b, name);
        (p.log.len(), p.ctl.splits_max)
    });
    let writes: usize = results.iter().map(|r| r.0).sum();
    let splits = results.iter().map(|r| r.1).max().unwrap();
    eprintln!(
        "{} songs and subtunes ({seconds} s each): {writes} writes placed exactly, at most {splits} renders a chip's block",
        jobs.len()
    );
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn a_main_loop_that_skips_passes_makes_late_writes_in_order() {
    let blocks_per_20ms = (SAMPLE_RATE / 50) as usize / BLOCK;
    let mut total_late = 0;
    for (n, (name, sng)) in songs().into_iter().take(6).enumerate() {
        for sid in [0xD400, 0xD420] {
            let file = packed(&sng, sid);
            let mut p = Player::new(&file);
            assert!(p.play(1));
            let mut rng =
                Xorshift(0x9E37_79B9_7F4A_7C15 ^ (n as u64 * 2 + u64::from(sid == 0xD420)));
            let (mut a, mut b) = (Vec::new(), Vec::new());
            let mut stall = 0;
            for _ in 0..blocks_for(8) {
                if stall == 0 && rng.next().is_multiple_of(40) {
                    stall = 1 + (rng.next() % blocks_per_20ms as u64) as usize;
                }
                if stall > 0 {
                    stall -= 1;
                } else {
                    for _ in 0..1 + rng.next() % 6 {
                        p.pass();
                    }
                }
                let (sa, sb, _) = p.block();
                a.extend_from_slice(&sa);
                b.extend_from_slice(&sb);
                assert!(p.ahead_max <= AHEAD_BOUND, "{name}");
            }
            assert_eq!(p.ctl.dropped, 0);
            assert!(
                p.ctl.splits_max < PSID_SPLITS,
                "{name}: the budget must not be what moved writes"
            );
            let applied = p.as_applied();
            let moved = applied.iter().zip(&p.log).filter(|(x, y)| x != y).count();
            assert_eq!(moved as u32, p.late(), "{name}: late writes counted");
            assert_exact_against_a_reference_render(
                &p,
                &file,
                &a,
                &b,
                &format!("{name} at ${sid:04X}"),
            );
            total_late += p.late();
        }
    }
    assert!(total_late > 0, "the stalls made no write late");
    eprintln!("{total_late} late writes, each applied where expected");
}

fn init_writing_voice1_freq_lo(n: u32, gap: u32, value_of: impl Fn(u32) -> u8) -> Vec<u8> {
    let mut init = String::from("lda #$11\nsta $d404\nlda #$0f\nsta $d418\nlda #$f0\nsta $d406\n");
    for i in 0..n {
        init += &format!("lda #${:02x}\nsta $d400\n", value_of(i));
        init += &exact(gap - LDA_IMM_STA_ABS_CYCLES);
    }
    init += "rts\n";
    Psid::new(&[(INIT, &init), (LATE_PLAY, "rts\n")]).bytes()
}

const LATE_PLAY: u16 = 0x8000;

fn assert_same_writes(got: &[Write], want: &[Write], what: &str) {
    let first = first_difference(got, want);
    assert_eq!(
        (first, got.len()),
        (None, want.len()),
        "{what}: the writes differ ({:?} against {:?})",
        first.map(|i| got[i]),
        first.map(|i| want[i])
    );
}

#[test]
fn writes_past_the_split_budget_go_in_order_at_the_blocks_end() {
    let file = init_writing_voice1_freq_lo(400, 40, |i| (i * 37 + 5) as u8);
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let blocks = 12;
    let (a, _) = p.run(blocks, PASSES);
    assert_eq!(p.ctl.splits_max, PSID_SPLITS);
    let mut chips = chips_for(&header(&file));
    let starts: Vec<u32> = p.blocks.iter().map(|s| s.0).collect();
    let mut expected = p.log.clone();
    let cps = chips[0].cycles_per_sample_q16;
    let mut late = 0;
    {
        let (mut q, mut now) = (chips[0].cycle_frac_q16, 0u32);
        let mut w = 0;
        for (b, &start) in starts.iter().enumerate() {
            assert_eq!(start, now, "block {b}'s start");
            let mut cycles = Vec::with_capacity(BLOCK);
            for _ in 0..BLOCK {
                q += cps;
                now += q >> 16;
                q &= 0xffff;
                cycles.push(now);
            }
            let end = now;
            let (mut renders, mut samples_rendered) = (0, 0usize);
            while w < expected.len() && expected[w].cycle < end {
                let x = expected[w].cycle;
                if renders < PSID_SPLITS - 1 {
                    let ending_by_the_write = cycles[samples_rendered..]
                        .iter()
                        .take_while(|&&c| c <= x)
                        .count();
                    if ending_by_the_write > 0 {
                        renders += 1;
                        samples_rendered += ending_by_the_write;
                    }
                } else {
                    expected[w].cycle = end;
                    late += 1;
                }
                w += 1;
            }
        }
    }
    assert_eq!(p.late(), late);
    assert!(late > 100, "{late} writes past the budget");
    let (ra, _) = reference(&mut chips, &expected, p.blocks.len());
    assert_eq!(
        first_difference(since_the_audio_started(&p, &a), &ra),
        None,
        "the render with the budget's writes at the blocks' ends"
    );
}

#[test]
fn dense_bursts_of_a_players_writes_land_on_time() {
    for (n, gap) in [(26, 15), (23, 18), (26, 17)] {
        let file = init_writing_voice1_freq_lo(n, gap, |i| (i * 37 + 5) as u8);
        let mut p = Player::new(&file);
        assert!(p.play(1));
        let (a, b) = p.run(blocks_for(1), PASSES);
        assert!(
            p.ctl.splits_max > 16,
            "{n} writes {gap} cycles apart: {} renders a block, the burst is no more than 16 \
             parts",
            p.ctl.splits_max
        );
        assert!(p.ctl.splits_max <= PSID_SPLITS);
        assert_eq!(p.late(), 0, "{n} writes {gap} cycles apart: late writes");
        assert_eq!(p.log.len(), n as usize + 3, "the burst and init's setup");
        assert_exact_against_a_reference_render(&p, &file, &a, &b, "a dense burst");
    }
}

#[test]
fn a_slow_init_plays_in_real_time_from_the_audios_start() {
    let init = format!(
        "lda #$11\nsta $d404\nlda #$0f\nsta $d418\nlda #$40\nsta <$fc\n\
         s_loop: lda <$fc\nsta $d401\n{}dec <$fc\nbne s_loop\nrts\n",
        exact(30_000)
    );
    let file = Psid::new(&[(INIT, &init), (LATE_PLAY, &play_src())]).bytes();
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let period = PSID_FRAME_PAL;
    let (mut silent, mut all, mut allb) = (0, Vec::new(), Vec::new());
    loop {
        for _ in 0..PASSES {
            p.pass();
        }
        let now = p.work.env.cpu.cycles;
        let (sa, sb, r) = p.block();
        if r != 0 {
            assert!(now >= period, "started at {now}");
            all.extend_from_slice(&sa);
            allb.extend_from_slice(&sb);
            break;
        }
        silent += 1;
        assert!(silent < 100);
    }
    assert_eq!(p.state(), PSID_INIT);
    let (a, b) = p.run(blocks_for(3), PASSES);
    assert_eq!(p.state(), PSID_PLAYING, "init returned");
    assert_eq!(p.late(), 0);
    all.extend_from_slice(&a);
    allb.extend_from_slice(&b);
    assert!(
        p.log.iter().filter(|w| w.reg == 1).count() >= 64,
        "init's writes"
    );
    assert_exact_against_a_reference_render(&p, &file, &all, &allb, "slow init");
    assert_eq!(p.blocks[0].0, 0, "the audio starts at the tune's cycle 0");
}

fn song_file(n: usize, sid: u32) -> Vec<u8> {
    let (_, sng) = &songs()[n];
    packed(sng, sid)
}

fn plain_run_chip_a(file: &[u8], blocks: usize) -> Vec<i16> {
    let mut p = Player::new(file);
    assert!(p.play(1));
    p.run(blocks, PASSES).0
}

fn ramp(x: &[i16], down: bool) -> Vec<i16> {
    x.iter()
        .enumerate()
        .map(|(f, &v)| {
            let k = if down { BLOCK - 1 - f } else { f } as i32;
            ((i32::from(v) * k) >> 7) as i16
        })
        .collect()
}

#[test]
fn pause_and_resume_give_the_same_samples_shifted() {
    let file = song_file(0, 0xD400);
    let total = 400;
    let want = plain_run_chip_a(&file, total);
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let (got, _) = p.run(150, PASSES);
    assert!(p.pause());
    assert_eq!(p.state(), PSID_PAUSED);
    let (paused, _) = p.run(60, PASSES);
    assert!(paused[BLOCK..].iter().all(|&x| x == 0));
    assert!(p.ahead() <= AHEAD_BOUND);
    assert!(p.resume());
    assert_eq!(p.state(), PSID_PLAYING);
    let (after, _) = p.run(total - 151, PASSES);
    assert_eq!(p.late(), 0);
    let at = 150 * BLOCK;
    assert_eq!(&got[..], &want[..at], "before the pause");
    assert_eq!(
        &paused[..BLOCK],
        &ramp(&want[at..at + BLOCK], true)[..],
        "the fade out"
    );
    assert_eq!(
        &after[..BLOCK],
        &ramp(&want[at + BLOCK..at + 2 * BLOCK], false)[..],
        "the fade in"
    );
    assert_eq!(&after[BLOCK..], &want[at + 2 * BLOCK..], "after the resume");
}

#[test]
fn a_stop_fades_clears_the_chips_and_their_mutes() {
    let file = song_file(1, 0xD400);
    let want = plain_run_chip_a(&file, 200);
    let mut p = Player::new(&file);
    assert!(p.mute(0b010));
    assert!(p.play(1));
    assert_eq!(p.chips[0].mute, 0b010, "the mutes at the song's start");
    assert!(p.mute(0));
    let (got, _) = p.run(120, PASSES);
    assert_eq!(&got[..], &want[..120 * BLOCK]);
    assert!(p.mute(0b100));
    assert_eq!(p.chips[0].mute, 0b100);
    assert!(p.stop());
    assert_eq!(p.state(), PSID_STOPPED);
    assert!(!p.play(1), "not until the stop has played out");
    let (sa, _, r) = p.block();
    assert_eq!(r, 1);
    let mut q = Player::new(&file);
    assert!(q.play(1));
    q.run(120, PASSES);
    q.mute(0b100);
    q.pass();
    let (qa, _, _) = q.block();
    assert_eq!(&sa[..], &ramp(&qa, true)[..], "the stop's fade");
    for v in 0..3 {
        assert_eq!(p.chips[0].osc[v].freq, 0, "voice {v}'s frequency");
        assert_eq!(p.chips[0].osc[v].pw, 0);
    }
    assert_eq!(p.chips[0].mode_vol, 0);
    assert_eq!(
        p.chips[0].mute, 0b100,
        "until the poll that sees the stop played"
    );
    p.pass();
    assert_eq!(p.chips[0].mute, 0, "the stop clears the mutes");
    let (s, _, r) = p.block();
    assert_eq!(r, 0, "stopped: not rendered");
    assert!(s.iter().all(|&x| x == 0));
    let clock = p.ctl.clock[0];
    p.run(10, PASSES);
    assert_eq!(p.ctl.clock[0], clock, "the chip's clock stands still");
    assert!(p.play(1));
    assert_eq!(p.state(), PSID_INIT);
}

#[test]
fn a_song_change_starts_init_again() {
    let (name, sng) = songs()
        .into_iter()
        .find(|(_, s)| gt2_ref::subtunes(s) >= 2)
        .expect("a song with subtunes");
    let file = packed(&sng, 0xD400);
    let mut p = Player::new(&file);
    assert!(p.play(1));
    p.run(100, PASSES);
    assert!(p.subtune(2));
    assert_eq!(p.state(), PSID_STOPPED);
    let (sa, _, r) = p.block();
    assert_eq!(r, 1);
    assert!(sa.iter().any(|&x| x != 0));
    let chips = Box::new([p.chips[0], p.chips[1]]);
    p.pass();
    assert!(matches!(p.state(), PSID_INIT | PSID_PLAYING), "{name}");
    assert_eq!(p.work.env.song, 2);
    assert!(
        p.work.env.cpu.cycles <= PSID_SLICE_CYCLES + 7,
        "one slice of song 2"
    );
    assert_eq!(p.work.env.calls, 0);
    let (a, b) = p.run(300, PASSES);
    assert_eq!(p.state(), PSID_PLAYING);
    assert_eq!(p.late(), 0);
    let s = stream(&file, 2, p.log.last().unwrap().cycle + 1);
    assert_same_writes(&p.log, &s, "song 2");
    let mut c = chips;
    let (ra, _) = reference(&mut c, &p.as_applied(), p.blocks.len());
    assert_eq!(since_the_audio_started(&p, &a), &ra[..]);
    let _ = b;
}

fn two_chip_tune() -> Vec<u8> {
    let init = "lda #$0f\nsta $d418\nsta $d438\nlda #$09\nsta $d405\nsta $d425\n\
                lda #$f0\nsta $d406\nsta $d426\nlda #$08\nsta $d423\n\
                lda #$21\nsta $d404\nlda #$41\nsta $d424\nrts\n";
    let play = "inc <$fb\nlda <$fb\nsta $d401\nasl\nsta $d421\nsta $d400\nrts\n";
    Psid::new(&[(INIT, init), (PLAY, play)]).sid2(0x42).bytes()
}

#[test]
fn a_2sid_tune_places_both_chips_writes_and_mutes_both() {
    let file = two_chip_tune();
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let (a, b) = p.run(300, PASSES);
    assert_eq!(p.late(), 0);
    assert!(p.log.iter().any(|w| w.chip == 0) && p.log.iter().any(|w| w.chip == 1));
    assert!(a.iter().any(|&x| x != 0) && b.iter().any(|&x| x != 0));
    assert_exact_against_a_reference_render(&p, &file, &a, &b, "2SID");
    assert!(p.mute(0b001_001));
    assert_eq!((p.chips[0].mute, p.chips[1].mute), (1, 1));
    let (a2, b2) = p.run(100, PASSES);
    let quiet = |x: &[i16]| x.iter().map(|&v| i32::from(v).abs()).max().unwrap();
    assert!(quiet(&a2[20 * BLOCK..]) < quiet(&a) / 4);
    assert!(quiet(&b2[20 * BLOCK..]) < quiet(&b) / 4);
    assert!(p.stop());
    p.block();
    p.pass();
    assert_eq!((p.chips[0].mute, p.chips[1].mute), (0, 0));
}

#[test]
fn a_1sid_tune_leaves_chip_b_alone() {
    let file = song_file(2, 0xD400);
    let mut p = Player::new(&file);
    let before = p.chips[1];
    assert!(p.mute(0b111_111));
    assert!(p.play(1));
    p.run(50, PASSES);
    assert_eq!(p.chips[1].mute, 0, "chip B's mute is the keys'");
    assert_eq!(
        p.chips[1].cycle_frac_q16, before.cycle_frac_q16,
        "chip B never rendered"
    );
    assert_eq!(p.chips[1].osc[0].acc, before.osc[0].acc);
}

fn init_writing_voice1_freq_lo_for_ever(gap: u32) -> Vec<u8> {
    let init = format!(
        "lda #$11\nsta $d404\nlda #$0f\nsta $d418\nloop: sta $d400\n{}clc\nadc #$07\njmp loop\n",
        exact(gap - STA_ABS_CLC_ADC_IMM_JMP_CYCLES)
    );
    Psid::new(&[(INIT, &init), (LATE_PLAY, "rts\n")]).bytes()
}

#[test]
fn the_ring_carries_writes_across_its_wrap() {
    let file = init_writing_voice1_freq_lo_for_ever(240);
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let x = 0u32.wrapping_sub(3000);
    p.ring_from(x);
    let (a, b) = p.run(blocks_for(3), PASSES);
    assert!(
        p.ctl.tail < x && p.ctl.tail > 9_000,
        "the indexes wrapped: {}",
        p.ctl.tail
    );
    assert_eq!(p.ctl.dropped, 0);
    assert_eq!(p.late(), 0);
    assert!(p.log.len() > 12_000);
    assert_same_writes(
        &p.log,
        &stream(&file, 1, p.log.last().unwrap().cycle + 1),
        "the wrap",
    );
    assert_exact_against_a_reference_render(&p, &file, &a, &b, "across the wrap");
}

#[test]
fn a_write_every_four_cycles_never_overflows_the_ring() {
    let mut init = String::from("lda #$11\nsta $d404\nloop:\n");
    for _ in 0..200 {
        init += "sta $d400\n";
    }
    init += "jmp loop\n";
    let file = Psid::new(&[(INIT, &init), (LATE_PLAY, "rts\n")]).bytes();
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let blocks = blocks_for(1);
    p.run(blocks, PASSES);
    assert_eq!(p.ctl.dropped, 0);
    let taken = p.ctl.tail;
    assert!(
        taken <= blocks as u32 * PSID_BLOCK_ENTRIES
            && taken > (blocks as u32 - 8) * PSID_BLOCK_ENTRIES,
        "{taken} entries in {blocks} blocks"
    );
    assert!(p.late() > 0, "the budget is spent in every block");
    assert!(
        (14..=PSID_SPLITS).contains(&p.ctl.splits_max),
        "{} renders",
        p.ctl.splits_max
    );
}

#[test]
fn a_tune_that_hangs_fails_and_stops() {
    let file = play_forever().bytes();
    let mut p = Player::new(&file);
    assert!(p.play(1));
    let mut n = 0;
    while p.state() != PSID_FAILED {
        p.run(1, PASSES);
        n += 1;
        assert!(n < 2000, "no PLAY HUNG");
    }
    assert_eq!(p.ctl.error as u32, PSID_RUN_PLAY_HUNG);
    p.run(2, PASSES);
    assert_eq!(p.ctl.mode, PSID_ISR_OFF as u8);
    assert!(p.play(1), "it can be played again");
}

#[test]
fn the_structs_fit_the_overlays() {
    eprintln!(
        "psid_ctl {} bytes (ring {}), psid_work {}",
        ilp32::psid_ctl::SIZE,
        PSID_RING * 6,
        ilp32::psid_work::SIZE
    );
    assert_eq!(ilp32::psid_ctl::SIZE, 6256);
    assert_eq!(
        ilp32::psid_work::SIZE,
        ilp32::psid_env::SIZE + 2 * ilp32::sid_shadow::SIZE
    );
}
