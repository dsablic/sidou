// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song transport from Rust (M2c Task 6): `core/player/song_ctl.h`'s main-loop and
//! audio-interrupt sides on the host, run as the device runs them (a block's commands, its
//! render split at the call, a main-loop pass between blocks).

use sidou_sys::*;

use crate::song::Song;

/// The samples a block (platform/fm1/audio.h AUDIO_HALF_FRAMES).
pub const BLOCK: usize = SONG_BLOCK as usize;
/// The device's sample rate (24 MHz / 544, rounded).
pub const SAMPLE_RATE: u32 = 44_118;

/// song_ctl over a song, with its shape buffers, its work space and chip A.
pub struct Transport {
    pub sc: Box<song_ctl>,
    _shapes: Box<[gt2_shape; 2]>,
    _work: Box<song_work>,
    pub chip: Box<sid>,
}

impl Transport {
    /// The song must outlive the transport and stay where it is (its region is on the heap).
    pub fn new(song: &mut Song) -> Transport {
        let mut sc = Box::<song_ctl>::default();
        let mut shapes = Box::<[gt2_shape; 2]>::default();
        let mut work = Box::<song_work>::default();
        let mut chip = Box::<sid>::default();
        // SAFETY: everything song_ctl keeps a pointer to is boxed and owned by the transport,
        // or is the song, which the caller keeps.
        unsafe {
            sid_init(&mut *chip, SID_MODEL_6581, SID_CLOCK_PAL, SAMPLE_RATE);
            let (a, b) = shapes.split_at_mut(1);
            song_ctl_init(&mut *sc, song.ptr_mut(), &mut a[0], &mut b[0], &mut *work);
        }
        Transport {
            sc,
            _shapes: shapes,
            _work: work,
            chip,
        }
    }

    /// The audio interrupt's block: the commands, chip A's render split at the call, the
    /// block's end. The call's sample, if one fell in the block.
    pub fn block(&mut self, out: &mut [i16]) -> Option<usize> {
        let n = out.len() as u32;
        unsafe {
            song_ctl_block(&mut *self.sc, &mut *self.chip);
            let at = song_ctl_due(&*self.sc, n);
            song_ctl_render(&mut *self.sc, &mut *self.chip, out.as_mut_ptr(), n);
            song_ctl_block_end(&mut *self.sc, n);
            (at < n).then_some(at as usize)
        }
    }

    /// A block whose samples are dropped.
    pub fn block_silent(&mut self) -> Option<usize> {
        let mut out = [0i16; BLOCK];
        self.block(&mut out)
    }

    /// A main-loop pass: what the interrupt applied, a slice, the commands made visible.
    pub fn pass(&mut self) {
        unsafe {
            song_ctl_poll(&mut *self.sc, SONG_POLL_SLICE | SONG_POLL_RING);
            song_ctl_commit(&mut *self.sc);
        }
    }

    /// Passes and blocks, one each, until `done` or `limit` blocks.
    pub fn run_until(&mut self, limit: usize, mut done: impl FnMut(&Transport) -> bool) -> bool {
        for _ in 0..limit {
            if done(self) {
                return true;
            }
            self.pass();
            self.block_silent();
        }
        done(self)
    }

    pub fn state(&self) -> u32 {
        unsafe { song_ctl_state(&*self.sc) }
    }

    pub fn play(&mut self, subtune: u32, pos: u32) -> bool {
        unsafe { song_ctl_play(&mut *self.sc, subtune, pos) != 0 }
    }

    pub fn stop(&mut self) -> bool {
        unsafe { song_ctl_stop(&mut *self.sc) != 0 }
    }

    pub fn resume(&mut self) -> bool {
        unsafe { song_ctl_continue(&mut *self.sc) != 0 }
    }

    pub fn mute(&mut self, mask: u32) -> bool {
        unsafe { song_ctl_mute(&mut *self.sc, mask) != 0 }
    }

    pub fn take(&mut self, mask: u32) -> bool {
        unsafe { song_ctl_take(&mut *self.sc, mask) != 0 }
    }

    pub fn jam(&mut self, voice: u32, note: u32, instr: u32) -> bool {
        unsafe { song_ctl_jam(&mut *self.sc, voice, note, instr) != 0 }
    }

    /// The audition: jams with the song stopped (M2d).
    pub fn audition(&mut self) -> bool {
        unsafe { song_ctl_audition(&mut *self.sc) != 0 }
    }

    pub fn audition_end(&mut self) -> bool {
        unsafe { song_ctl_audition_end(&mut *self.sc) != 0 }
    }

    pub fn jam_ready(&self) -> bool {
        unsafe { song_ctl_jam_ready(&*self.sc) != 0 }
    }

    pub fn edit(&mut self, e: &gt2_edit) -> bool {
        unsafe { song_ctl_edit(&mut *self.sc, e) != 0 }
    }

    pub fn seek(&mut self, call: u32) -> bool {
        unsafe { song_ctl_seek(&mut *self.sc, call) != 0 }
    }

    pub fn events(&mut self) -> Vec<song_event> {
        let mut out = vec![song_event::default(); SONG_EVENT_RING as usize];
        let n = unsafe { song_ctl_events(&mut *self.sc, out.as_mut_ptr(), out.len() as u32) };
        out.truncate(n as usize);
        out
    }

    /// The interrupt's play routine calls since the play.
    pub fn calls(&self) -> u32 {
        self.sc.calls
    }

    /// The shape the interrupt's player follows.
    pub fn shape(&self) -> &gt2_shape {
        unsafe { &*self.sc.player.shape }
    }
}

/// A player's state as bytes, its pointers and start positions left out: two players that
/// would make the same writes from here on have the same bytes.
pub fn player_state(p: &gt2_player) -> Vec<u8> {
    let mut q = *p;
    q.song = std::ptr::null();
    q.shape = std::ptr::null();
    q.start = [0; 3];
    // up to the last field (the struct's tail padding is not data)
    let bytes = std::mem::offset_of!(gt2_player, ev) + size_of::<[gt2_event; 3]>();
    // SAFETY: gt2_player is plain data, and these bytes are its fields.
    unsafe { std::slice::from_raw_parts((&q as *const gt2_player).cast::<u8>(), bytes) }.to_vec()
}
