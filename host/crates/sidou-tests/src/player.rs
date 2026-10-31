// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::os::raw::c_void;

use sidou_sys::*;

use crate::song::Song;

pub type Writes = Vec<(u8, u8)>;

pub struct Player {
    p: Box<gt2_player>,
    shape: Box<gt2_shape>,
}

unsafe extern "C" fn collect(ctx: *mut c_void, reg: u8, value: u8) {
    let w = unsafe { &mut *(ctx as *mut Writes) };
    w.push((reg, value));
}

unsafe extern "C" fn discard(_ctx: *mut c_void, _reg: u8, _value: u8) {}

pub fn default_options() -> gt2_options {
    let mut o = gt2_options::default();
    unsafe { gt2_options_default(&mut o) };
    o
}

impl Player {
    pub fn new(song: &Song, subtune: u32, o: &gt2_options) -> Option<Player> {
        Player::start(song, subtune, o).ok()
    }

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

    pub fn call(&mut self) -> Writes {
        let mut w = Writes::new();
        unsafe { gt2_play_frame(&mut *self.p, Some(collect), (&mut w as *mut Writes).cast()) };
        w
    }

    pub fn call_silent(&mut self) {
        unsafe { gt2_play_frame(&mut *self.p, Some(discard), std::ptr::null_mut()) };
    }

    pub fn raw(&self) -> &gt2_player {
        &self.p
    }

    pub fn shape(&self) -> &gt2_shape {
        &self.shape
    }
}
