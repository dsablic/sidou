// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The song player from Rust (M2c Tasks 4-5b): a safe wrapper over `core/player`.

use std::os::raw::c_void;

use sidou_sys::*;

use crate::song::Song;

/// A call's SID writes: (register 0-24, value), in order.
pub type Writes = Vec<(u8, u8)>;

/// A player over a song and the shape its analysis gave it; the song must outlive it (it
/// reads the song every call).
pub struct Player {
    p: Box<gt2_player>,
    shape: Box<gt2_shape>,
}

unsafe extern "C" fn collect(ctx: *mut c_void, reg: u8, value: u8) {
    let w = unsafe { &mut *(ctx as *mut Writes) };
    w.push((reg, value));
}

unsafe extern "C" fn discard(_ctx: *mut c_void, _reg: u8, _value: u8) {}

/// GoatTracker's default options (gt2reloc's).
pub fn default_options() -> gt2_options {
    let mut o = gt2_options::default();
    unsafe { gt2_options_default(&mut o) };
    o
}

impl Player {
    /// The player after `init` with the subtune, or None if the packed song would not hold
    /// it.
    pub fn new(song: &Song, subtune: u32, o: &gt2_options) -> Option<Player> {
        Player::start(song, subtune, o).ok()
    }

    /// The player, or gt2_play_init's refusal (`GT2_PLAY_ERR_*`).
    pub fn start(song: &Song, subtune: u32, o: &gt2_options) -> Result<Player, i32> {
        let mut p = Box::new(gt2_player::default());
        let mut shape = Box::new(gt2_shape::default());
        let r = unsafe { gt2_play_init(&mut *p, &mut *shape, song.ptr(), subtune, o) };
        if r == 0 {
            Ok(Player { p, shape })
        } else {
            Err(r)
        }
    }

    /// One call of the play routine.
    pub fn call(&mut self) -> Writes {
        let mut w = Writes::new();
        unsafe { gt2_play_frame(&mut *self.p, Some(collect), (&mut w as *mut Writes).cast()) };
        w
    }

    /// One call, its writes dropped (for timing).
    pub fn call_silent(&mut self) {
        unsafe { gt2_play_frame(&mut *self.p, Some(discard), std::ptr::null_mut()) };
    }

    pub fn raw(&self) -> &gt2_player {
        &self.p
    }

    /// The shape the player follows.
    pub fn shape(&self) -> &gt2_shape {
        &self.shape
    }
}
