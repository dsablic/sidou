// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! SIDOU's first song: assets/songs/sidou-loop.sng and app/demo_song.c are generated from
//! sidou_tests::song_embed; SIDOU_SONG_UPDATE=1 rewrites them.

use sidou_sys::*;
use sidou_tests::song::Song;
use sidou_tests::song_embed;

#[test]
fn the_generated_files_are_up_to_date() {
    let update = std::env::var("SIDOU_SONG_UPDATE").as_deref() == Ok("1");
    let stale = song_embed::check_or_write(update);
    assert!(
        stale.is_empty(),
        "{stale:?} is stale; rerun with SIDOU_SONG_UPDATE=1"
    );
    for line in song_embed::c_source(&song_embed::sng()).lines() {
        assert!(
            line.len() <= 100,
            "generated line over 100 columns: {line:?}"
        );
    }
}

/// The firmware's copy is the song's, and the device reads it (its caps, its 64 KiB
/// region).
#[test]
fn the_firmwares_song_reads_on_the_device() {
    // SAFETY: app/demo_song.c defines the array with demo_song_sng_bytes bytes.
    let embedded = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!(demo_song_sng).cast::<u8>(),
            demo_song_sng_bytes as usize,
        )
    };
    assert_eq!(embedded, &song_embed::sng()[..]);
    let mut s = Song::device();
    let info = s.read(embedded, 4).expect("reads with the device's caps");
    assert_eq!(info.channels, 3);
}

/// GoatTracker's own load and save leave it as it is: a canonical GTS5 file (decision
/// record 0007), so it opens in GoatTracker and comes back byte for byte.
#[test]
fn the_song_is_canonical_gts5() {
    let sng = song_embed::sng();
    assert_eq!(&sng[..4], b"GTS5");
    assert_eq!(gt2_ref::canonical(&sng).expect("GoatTracker loads it"), sng);
}

unsafe extern "C" fn count_writes(ctx: *mut std::ffi::c_void, _reg: u8, _value: u8) {
    unsafe { *(ctx as *mut u32) += 1 };
}

unsafe extern "C" fn whole(_ctx: *mut std::ffi::c_void, _profile: u8) -> i32 {
    1
}

/// The ring's writes for the song's chips in one pass, at most (app/keys_route.h's contract):
/// under KEYS A both chips follow a setup in a song's tick (chip A the song's, asked for with
/// keys_route_song_wants by platform/fm1/main.c song_chip_ready; chip B the keys'), each one
/// chip_ctl_follow at its worst (measured here: a CHPA with another profile and every control,
/// the tick its preparation ends in), and chip A's character tick besides: within the
/// REG_RING_SONG_PASS entries app/reg_ring.h keeps for them; a move of the keys
/// (REG_RING_MOVE_PASS) has its own share, so the shared pass is within REG_RING_WORST_PASS.
/// Song control itself puts nothing in the ring: PLAY, the stop's gates and the mute are the
/// interrupt's, and the stop hands chip A back through keys_route (its move's share).
#[test]
fn song_controls_pass_fits_its_ring_provision() {
    let mut c = Box::<chip_ctl>::default();
    let mut n = 0u32;
    let ctx: *mut u32 = &mut n;
    let mut worst = 0;
    unsafe {
        chip_ctl_init(
            &mut *c,
            Some(count_writes),
            ctx.cast(),
            Some(whole),
            std::ptr::null_mut(),
        );
        let keys = chip_setup {
            profile: SID_CHIP_6581_R4AR as u8,
            controls: [0; 4],
            gate_reset: 1,
        };
        while chip_ctl_follow(&mut *c, &keys) == 0 {}
        let song = chip_setup {
            profile: SID_CHIP_8580_R5 as u8,
            controls: [100, 50, 20, 127],
            gate_reset: 0,
        };
        loop {
            n = 0;
            let done = chip_ctl_follow(&mut *c, &song);
            worst = worst.max(n);
            if done != 0 {
                break;
            }
        }
    }
    // both chips following, and chip A's character tick on the song's tick
    let shared = 2 * worst + 1;
    assert!(
        shared <= REG_RING_SONG_PASS,
        "{worst} entries a follow, {shared} shared"
    );
    assert!(REG_RING_LIVE_PASS + shared + REG_RING_MOVE_PASS <= REG_RING_WORST_PASS);
    println!(
        "a follow's worst: {worst} ring entries; both chips and chip A's tick: {shared} of \
         {REG_RING_SONG_PASS}"
    );
}
