// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The firmware's step 1 of the update protocol (Felucca's ota.c, built for the host by
//! harness/ota_harness.c) against packages from this crate, with this test as the installer
//! (fm1_install.py's side: identity, read requests, "success"). fm1-emu has no USB, so this
//! is where the protocol and the package format meet before hardware.
//!
//! Offline, with stand-in SDK files; the device's own head is a synthetic one (the chip key
//! in an isd_config blob). With M-VAVE's V15 package in the cache (`build/stock/FM-1.fwsc`,
//! SHA-256 checked; only ever read by these tests, never put into a package), the device
//! carries V15's head as a real FM-1 does, and the way back to stock is tested too.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::{Mutex, MutexGuard};

use sidou_pack::harness::{ota_h_boot_cleanup, ota_h_service, ota_h_session, OtaHost};
use sidou_pack::{
    crc16, default_cache, enc, logical_image, sha256_hex, ufw, ufw_file, wrap_loader, Inputs,
    CHIP_KEY, HEAD_SIZE,
};

const FLASH: usize = 1 << 20;
const OTA_AREA: usize = 0xE0000;
const OTA_RES: usize = 0xE4F00;
const PRODUCT: &str = "FM-1_900";

/// ota.c keeps static state and the harness one global device: one test at a time.
static HARNESS: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    HARNESS.lock().unwrap_or_else(|e| e.into_inner())
}

fn pack7(data: &[u8]) -> Vec<u8> {
    let (mut out, mut acc, mut nb) = (Vec::new(), 0u32, 0);
    for &b in data {
        acc |= u32::from(b) << nb;
        nb += 8;
        while nb >= 7 {
            out.push((acc & 0x7F) as u8);
            acc >>= 7;
            nb -= 7;
        }
    }
    if nb > 0 {
        out.push((acc & 0x7F) as u8);
    }
    out
}

fn unpack7(s: &[u8]) -> Vec<u8> {
    let (mut out, mut acc, mut nb) = (Vec::new(), 0u32, 0);
    for &b in s {
        acc |= u32::from(b & 0x7F) << nb;
        nb += 7;
        while nb >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            nb -= 8;
        }
    }
    out
}

/// The installer: answers the device's read requests from the package's logical image.
struct Installer {
    image: Vec<u8>,
    queue: VecDeque<Vec<u8>>,
    current: Vec<u8>,
    reads: Vec<(u32, u32)>,
    identity: Option<String>,
}

impl Installer {
    fn new(package: &[u8]) -> Box<Self> {
        Box::new(Installer {
            image: logical_image(package),
            queue: VecDeque::new(),
            current: Vec::new(),
            reads: Vec::new(),
            identity: None,
        })
    }

    /// fm1_install.py `response`: the frame between F0 and F7.
    fn respond(&mut self, addr: u32, data: &[u8]) {
        let n = data.len();
        let mut body = vec![0x00, 0x59, 0x30, (n + 8) as u8, ((n + 8) >> 8) as u8, 0, 0];
        body.extend_from_slice(&addr.to_le_bytes());
        body.extend_from_slice(&[n as u8, (n >> 8) as u8, 0]);
        body.extend_from_slice(data);
        let sum: u32 = body[6..].iter().map(|&b| u32::from(b)).sum();
        body.push(!(sum as u8));
        self.queue.push_back(pack7(&body));
    }

    fn receive(&mut self, msg: &[u8]) {
        assert!(msg.first() == Some(&0xF0) && msg.last() == Some(&0xF7));
        let d = unpack7(&msg[1..msg.len() - 1]);
        assert_eq!(&d[..2], &[0x00, 0x59]);
        match d[2] {
            0x11 => {
                let text: Vec<u8> = d[6..33].iter().copied().take_while(|&b| b != 0).collect();
                self.identity = Some(String::from_utf8(text).unwrap());
            }
            0x30 => {
                let addr = u32::from_le_bytes(d[7..11].try_into().unwrap());
                let len = u32::from_le_bytes([d[11], d[12], d[13], 0]);
                self.reads.push((addr, len));
                if addr == 0xE000_0000 {
                    self.respond(addr, b"success\0");
                } else {
                    let (a, n) = (addr as usize, len as usize);
                    let data = self.image[a..a + n].to_vec();
                    self.respond(addr, &data);
                }
            }
            c => panic!("unexpected command {c:#x}"),
        }
    }
}

unsafe extern "C" fn send(ctx: *mut c_void, p: *const u8, n: u32) -> i32 {
    let inst = unsafe { &mut *ctx.cast::<Installer>() };
    inst.receive(unsafe { std::slice::from_raw_parts(p, n as usize) });
    0
}

unsafe extern "C" fn frame(ctx: *mut c_void, p: *mut *const u8, n: *mut u32) -> i32 {
    let inst = unsafe { &mut *ctx.cast::<Installer>() };
    let Some(f) = inst.queue.pop_front() else {
        return 0;
    };
    inst.current = f;
    unsafe {
        *p = inst.current.as_ptr();
        *n = inst.current.len() as u32;
    }
    1
}

unsafe extern "C" fn frame_done(_ctx: *mut c_void) {}

/// A device: its flash (1 MiB, the head at 0) and the harness's state.
struct Device {
    flash: Vec<u8>,
    host: Box<OtaHost>,
}

impl Device {
    fn new(head: &[u8], installer: &mut Installer) -> Self {
        let mut flash = vec![0xFFu8; FLASH];
        flash[..head.len()].copy_from_slice(head);
        let mut d = Device {
            flash,
            host: Box::new(OtaHost {
                ctx: (installer as *mut Installer).cast(),
                send,
                frame,
                frame_done,
                flash: std::ptr::null_mut(),
                flash_len: FLASH as u32,
                now: 0,
                committed: 0,
                parm: [0; 112],
                shown_step: 0,
                shown_code: 0,
            }),
        };
        d.host.flash = d.flash.as_mut_ptr();
        d
    }

    fn session(&mut self) -> i32 {
        unsafe { ota_h_session(&mut *self.host) }
    }
}

/// A 32-byte JLFS entry (fm1pkg_make.py `entry`).
fn jlfs(dcrc: u16, off: u32, size: u32, flags: u8, resvd: u8, index: u16, name: &[u8]) -> [u8; 32] {
    let mut e = [0xFFu8; 32];
    e[2..4].copy_from_slice(&dcrc.to_le_bytes());
    e[4..8].copy_from_slice(&off.to_le_bytes());
    e[8..12].copy_from_slice(&size.to_le_bytes());
    e[12] = flags;
    e[13] = resvd;
    e[14..16].copy_from_slice(&index.to_le_bytes());
    e[16..16 + name.len()].copy_from_slice(name);
    e[16 + name.len()] = 0;
    let c = crc16(&e[2..], 0);
    e[..2].copy_from_slice(&c.to_le_bytes());
    e
}

/// A head with only what the package and the loader need: a flash header and an isd_config
/// entry whose blob decodes to the chip key (fm1pkg_make.py `key_blob`).
fn synthetic_head() -> Vec<u8> {
    let mut blob = [0x10u8; 32];
    for i in 0..16 {
        blob[16 + i] = blob[15 - i] ^ if CHIP_KEY >> i & 1 != 0 { 0x00 } else { 0xFF };
    }
    let mut head = vec![0xFFu8; 0x4000];
    head[0x100..0x120].copy_from_slice(&blob);
    head[0x120..0x122].copy_from_slice(&crc16(&blob, 0).to_le_bytes());
    let mut top = vec![0u8; 32];
    top.extend_from_slice(&jlfs(
        crc16(&blob, 0),
        0x100,
        34,
        0x02,
        0x80,
        0,
        b"isd_config.ini",
    ));
    top.extend_from_slice(&jlfs(
        0xFFFF, 0xFF000, 0x1000, 0x12, 0x01, 0xFFFF, b"key_mac",
    ));
    for o in (0..top.len()).step_by(32) {
        enc(&mut top[o..o + 32], 0xFFFF);
    }
    head[..top.len()].copy_from_slice(&top);
    head
}

fn inputs() -> Inputs {
    Inputs {
        uboot: vec![0x33; 14536],
        cfg_tool: vec![0x11; 383],
        eq_cfg_hw: vec![0x22; 655],
    }
}

fn app() -> Vec<u8> {
    let mut a = b"SIDOU test app ".to_vec();
    a.extend_from_slice(b"FM-1_900\0");
    a.extend((0..5000u32).map(|i| (i * 13) as u8));
    a
}

fn loader() -> Vec<u8> {
    let mut l = b"\x04\x88FELUCCA-LOADER-1".to_vec();
    l.extend((0..6000u32).map(|i| (i * 31 % 253) as u8));
    l
}

/// M-VAVE's official V15 package (the SHA-256 Felucca's installer pins), if the cache has it.
const STOCK_V15_SHA256: &str = "db1642b2b6fa5c2cccb11ffd13878068bb28601678d3644049f99dc40e7edb8a";

fn stock_v15() -> Option<Vec<u8>> {
    let path = default_cache().join("stock/FM-1.fwsc");
    match std::fs::read(&path) {
        Ok(d) if sha256_hex(&d) == STOCK_V15_SHA256 => Some(d),
        _ => {
            println!(
                "skipping: no V15 package at {} (CI fetches it)",
                path.display()
            );
            None
        }
    }
}

/// The head a real FM-1 carries: V15's (no package of ours ever writes it).
fn device_head(stock: &[u8]) -> Vec<u8> {
    ufw_file(stock, 0).unwrap().1[..HEAD_SIZE].to_vec()
}

fn package() -> Vec<u8> {
    sidou_pack::package(&inputs(), &app(), &loader(), PRODUCT).unwrap()
}

#[test]
fn the_identity_request_is_answered() {
    let _one = exclusive();
    let pkg = package();
    let mut inst = Installer::new(&pkg);
    // fm1_install.py HS_QUERY: F0 00 32 45 00 00 00 40 7F F7, the frame between F0 and F7.
    inst.queue
        .push_back(vec![0x00, 0x32, 0x45, 0x00, 0x00, 0x00, 0x40, 0x7F]);
    let mut dev = Device::new(&synthetic_head(), &mut inst);
    unsafe { ota_h_service(&mut *dev.host) };
    assert_eq!(inst.identity.as_deref(), Some(PRODUCT));
}

#[test]
fn a_package_stages_its_loader_and_commits() {
    let _one = exclusive();
    let pkg = package();
    let mut inst = Installer::new(&pkg);
    let mut dev = Device::new(&synthetic_head(), &mut inst);
    let rc = dev.session();
    assert_eq!(
        (rc, dev.host.committed),
        (0, 1),
        "step 1 failed: ota.c code {rc}"
    );
    let (_, ota) = ufw_file(&pkg, 100).unwrap();
    // The staged loader: a "LOADER.BIN" head, then ota.bin's body.
    let staged = &dev.flash[OTA_AREA..OTA_AREA + ota.len()];
    assert_eq!(&staged[16..26], b"LOADER.BIN");
    assert_eq!(
        u16::from_le_bytes([staged[0], staged[1]]),
        crc16(&staged[2..32], 0)
    );
    assert_eq!(&staged[32..], &ota[32..]);
    // The update record: in flash and handed to the commit, with its CRC and the loader's
    // identity.
    let parm = dev.host.parm;
    assert_eq!(&dev.flash[OTA_RES..OTA_RES + 112], &parm);
    assert_eq!(
        u16::from_le_bytes([parm[0], parm[1]]),
        crc16(&parm[2..80], 0)
    );
    assert_eq!(&parm[8..20], b"ota-FM-1_015");
    assert_eq!(
        u32::from_le_bytes(parm[72..76].try_into().unwrap()),
        OTA_AREA as u32
    );
    assert_eq!(
        inst.reads.last(),
        Some(&(0xE000_0000, 8)),
        "no confirmation asked"
    );
    // Nothing outside the staging area was written.
    assert_eq!(&dev.flash[..0x4000], synthetic_head().as_slice());
    assert!(dev.flash[0x4000..OTA_AREA].iter().all(|&b| b == 0xFF));
    assert!(dev.flash[OTA_AREA + 0x5000..].iter().all(|&b| b == 0xFF));
    println!("step 1: {} read requests", inst.reads.len());
    // The next boot finds the staging area and clears it.
    assert_eq!(unsafe { ota_h_boot_cleanup(&mut *dev.host) }, 1);
    assert!(dev.flash[OTA_AREA..OTA_AREA + 0x5000]
        .iter()
        .all(|&b| b == 0xFF));
}

#[test]
fn another_boards_package_is_refused_before_anything_is_erased() {
    let _one = exclusive();
    let inputs = inputs();
    let mut flash = sidou_pack::flash_image(&inputs, &app()).unwrap();
    flash.truncate(0x92000);
    let pkg = ufw(&flash, &wrap_loader(&loader()).unwrap(), PRODUCT).unwrap();
    let mut inst = Installer::new(&pkg);
    let mut dev = Device::new(&synthetic_head(), &mut inst);
    let before = dev.flash.clone();
    assert_eq!(dev.session(), -15);
    assert_eq!(dev.host.committed, 0);
    assert!(dev.flash == before, "flash written");
}

#[test]
fn a_loader_without_the_marker_is_refused() {
    let _one = exclusive();
    let mut ota = wrap_loader(&loader()).unwrap();
    // The marker sits in the first block's literals: spoil one of its bytes there (ota.c
    // checks the heads' CRCs, not the body's, before it looks for the marker).
    let at = ota
        .windows(16)
        .position(|w| w == b"FELUCCA-LOADER-1")
        .unwrap();
    ota[at] = b'G';
    let flash = sidou_pack::flash_image(&inputs(), &app()).unwrap();
    let pkg = ufw(&flash, &ota, PRODUCT).unwrap();
    let mut inst = Installer::new(&pkg);
    let mut dev = Device::new(&synthetic_head(), &mut inst);
    let before = dev.flash.clone();
    assert_eq!(dev.session(), -7);
    assert!(dev.flash == before, "flash written");
}

#[test]
fn a_device_with_the_stock_head_takes_our_package() {
    let _one = exclusive();
    let Some(stock) = stock_v15() else {
        return;
    };
    let pkg = package();
    let mut inst = Installer::new(&pkg);
    let mut dev = Device::new(&device_head(&stock), &mut inst);
    assert_eq!(dev.session(), 0);
    assert_eq!(
        &dev.flash[..HEAD_SIZE],
        device_head(&stock).as_slice(),
        "the head was written"
    );
}

#[test]
fn the_way_back_to_stock_v15_stays_open() {
    let _one = exclusive();
    let Some(stock) = stock_v15() else {
        return;
    };
    // V15's package carries the official loader, which rewrites the head: ota.c lets it in only
    // when its head is the device's, which ours never changed.
    let mut inst = Installer::new(&stock);
    let mut dev = Device::new(&device_head(&stock), &mut inst);
    let rc = dev.session();
    println!(
        "V15 from SIDOU: ota.c code {rc}, {} read requests",
        inst.reads.len()
    );
    assert_eq!(
        (rc, dev.host.committed),
        (0, 1),
        "the return to V15 was refused"
    );
    // Had a tool written another head (say an SDK-default one), V15 would be refused before
    // anything is erased.
    let mut inst = Installer::new(&stock);
    let other = sidou_pack::head(&[0x33; 14536], CHIP_KEY).unwrap();
    let mut dev = Device::new(&other, &mut inst);
    let before = dev.flash.clone();
    assert_eq!(dev.session(), -5);
    assert!(dev.flash == before, "flash written");
}

/// Felucca's loader as the fm1 build makes it (`platform/fm1/loader/loader.bin`), if built.
fn built_loader() -> Option<Vec<u8>> {
    let dir = std::env::var_os("SIDOU_FM1_BUILD")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| default_cache().join("fm1"));
    let path = dir.join("platform/fm1/loader/loader.bin");
    let l = std::fs::read(&path).ok();
    if l.is_none() {
        println!("skipping: no {} (build the fm1 preset)", path.display());
    }
    l
}

#[test]
fn the_built_loader_is_staged() {
    let _one = exclusive();
    let Some(l) = built_loader() else {
        return;
    };
    let pkg = sidou_pack::package(&inputs(), &app(), &l, PRODUCT).unwrap();
    let mut inst = Installer::new(&pkg);
    let mut dev = Device::new(&synthetic_head(), &mut inst);
    assert_eq!((dev.session(), dev.host.committed), (0, 1));
}
