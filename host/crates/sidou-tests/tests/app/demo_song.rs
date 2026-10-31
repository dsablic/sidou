// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
            expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
        };
        while chip_ctl_follow(&mut *c, &keys) == 0 {}
        let song = chip_setup {
            profile: SID_CHIP_8580_R5 as u8,
            controls: [100, 50, 20, 127],
            gate_reset: 0,
            expert: [SID_EXPERT_DEFAULT as u8; SID_EXPERT_COUNT as usize],
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
