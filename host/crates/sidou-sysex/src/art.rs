// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

use sidou_sys::{art_pack_bytes, scene_rmr_art, ART_PACK_RMR_NAME};

use crate::{Error, FILE_ART, MAX_FILE_LEN};

pub const NAME: &str = match std::ffi::CStr::from_bytes_with_nul(ART_PACK_RMR_NAME) {
    Ok(name) => match name.to_str() {
        Ok(name) => name,
        Err(_) => panic!("the pack's store name is text"),
    },
    Err(_) => panic!("the pack's store name ends in its NUL"),
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Refusal(pub u32);

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            sidou_sys::ART_PACK_SHORT => "too short or over the store's 65,536 bytes a file",
            sidou_sys::ART_PACK_MAGIC => "not a SIDOU art pack (.sidart)",
            sidou_sys::ART_PACK_NEWER => "a newer art pack format than this SIDOU reads",
            sidou_sys::ART_PACK_SCENE => "art for another scene than RED MOON RITE",
            sidou_sys::ART_PACK_NAME => {
                "its pack name is not 1 to 16 characters of the screen font"
            }
            sidou_sys::ART_PACK_SLOTS => {
                "its slots are not RED MOON RITE's: every image at its size and frame count"
            }
            sidou_sys::ART_PACK_FRAME_BAD => "a frame is outside its slot or patches another size",
            sidou_sys::ART_PACK_ROWS => "a row table or a row is outside the pack or its page",
            sidou_sys::ART_PACK_CRC_BAD => "its CRC does not match its bytes",
            _ => "refused",
        })
    }
}

impl std::error::Error for Refusal {}

pub struct Pack {
    bytes: Vec<u8>,
    art: Box<scene_rmr_art>,
}

impl Pack {
    pub fn art(&self) -> &scene_rmr_art {
        &self.art
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn name(&self) -> String {
        self.bytes[16..32]
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as char)
            .collect()
    }
}

pub fn check(bytes: Vec<u8>) -> Result<Pack, Refusal> {
    if bytes.len() > MAX_FILE_LEN {
        return Err(Refusal(sidou_sys::ART_PACK_SHORT));
    }
    let mut art = Box::<scene_rmr_art>::default();
    // into bytes' heap buffer and art itself: both live, unmoved, as long as the Pack.
    let code = unsafe { art_pack_bytes(&mut *art, bytes.as_ptr(), bytes.len() as u32) };
    if code != 0 {
        return Err(Refusal(code as u32));
    }
    Ok(Pack { bytes, art })
}

pub fn syx(pack: &Pack) -> Result<Vec<u8>, Error> {
    Ok(crate::put_frames(NAME, FILE_ART, pack.bytes())?.concat())
}

const SECTOR: usize = sidou_sys::STORE_SECTOR as usize;
const ERASE_HEADER: usize = 20;

unsafe extern "C" fn seed_map(ctx: *mut std::ffi::c_void, off: u32) -> *const u8 {
    unsafe { ctx.cast::<u8>().add(off as usize) }
}

unsafe extern "C" fn seed_prog(
    ctx: *mut std::ffi::c_void,
    off: u32,
    p: *const std::ffi::c_void,
    n: u32,
) -> i32 {
    for i in 0..n as usize {
        unsafe { *ctx.cast::<u8>().add(off as usize + i) &= *p.cast::<u8>().add(i) };
    }
    sidou_sys::STORE_OK
}

unsafe extern "C" fn seed_erase(ctx: *mut std::ffi::c_void, off: u32) -> i32 {
    unsafe { std::ptr::write_bytes(ctx.cast::<u8>().add(off as usize), 0xff, SECTOR) };
    sidou_sys::STORE_OK
}

pub fn seed(pack: &Pack) -> Vec<u8> {
    use sidou_sys::*;
    let mut flash = vec![0xffu8; STORE_HI as usize];
    let hooks = store_flash {
        map: Some(seed_map),
        prog: Some(seed_prog),
        erase: Some(seed_erase),
        ctx: flash.as_mut_ptr().cast(),
    };
    let name = std::ffi::CString::new(NAME).expect("a C name");
    let bytes = pack.bytes();
    let mut st = Box::<store>::default();
    unsafe {
        store_init(&mut *st, &hooks, STORE_WINDOW_MAX);
        store_mount(&mut *st);
        store_quiet(&mut *st, 1);
        while store_poll(&mut *st) != 0 {}
        let begun = store_file_begin(&mut *st, name.as_ptr(), FILE_ART, bytes.len() as u32);
        assert_eq!(begun, STORE_OK, "a blank store takes the pack");
        let mut at = 0;
        while at < bytes.len() {
            at += store_file_write(
                &mut *st,
                bytes[at..].as_ptr().cast(),
                (bytes.len() - at) as u32,
            ) as usize;
            store_poll(&mut *st);
        }
        assert_eq!(store_file_end(&mut *st), STORE_OK);
        while store_file_result(&*st) == STORE_BUSY {
            store_poll(&mut *st);
        }
        assert_eq!(store_file_result(&*st), STORE_OK);
    }
    let mut out = Vec::new();
    for at in (STORE_LO as usize..STORE_HI as usize).step_by(SECTOR) {
        let sector = &flash[at..at + SECTOR];
        if sector[ERASE_HEADER..].iter().any(|&b| b != 0xff) {
            out.extend((at as u32).to_le_bytes());
            out.extend_from_slice(sector);
        }
    }
    out
}
