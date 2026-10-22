// SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-only
//! SIDOU's FM-1 update package (`.fwsc`): a port of Felucca's `tools/fm1pkg_make.py` and the
//! loader wrapping in its `tools/build.py` (Felucca a53ecf42, GPL-3.0-only, © 2026 Leo
//! Kuroshita). Its output is byte for byte what fm1pkg_make.py makes from the same inputs.
//!
//! The package (Felucca's layout, the JieLi SDK's file formats):
//! - `flash.bin`, the flash from 0 to the PRCT end (0x93000):
//!   - the head `[0, 0x4000)`: the JieLi SDK's default flash header, the JLFS entries, the
//!     SDK's SPL (`uboot.boot`) and an isd_config blob that decodes to the chip key. It only
//!     has to look like a package head: the update loader (Felucca's, SIDOU's) writes the
//!     app area only, so a device keeps its own head (M-VAVE's isd_config) whatever this
//!     one holds; the stock V15 package, on the way back, is checked against the device's
//!     head by ota.c;
//!   - the app area from 0x4000: `app_area_head`, `app.bin` (the firmware, padded to its
//!     slot), the SDK's `cfg_tool.bin`, the region descriptors, and a `cfg` directory with
//!     the SDK's default EQ table, scrambled per 32 bytes with the chip key (SFC);
//! - `ota.bin`, the update loader: two 32-byte JLFS heads, then the RAM image in LZ4 blocks of
//!   4096 bytes;
//! - in a `ufw` container whose first 20 blocks of 0x2F bytes are each followed by one marker
//!   byte: together they spell the package identity (`FM-1_900`).
//!
//! The inputs that are not ours, three AC79 SDK files (Apache-2.0), are fetched on first use
//! into a cache, checked against pinned SHA-256s (`inputs.toml`), and never committed. No
//! byte of M-VAVE's firmware goes into a package.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

pub mod lz4blk;

/// The app slot in the app area (Felucca's APP_SLOT): the firmware image is padded to it.
pub const APP_SLOT: usize = 0x8DFBC;
/// flash.bin: the head and this board's app area, up to the PRCT end.
pub const FLASH_SIZE: usize = 0x93000;
/// The flash head.
pub const HEAD_SIZE: usize = 0x4000;
/// The FM-1's chip key (the device head's isd_config decodes to it).
pub const CHIP_KEY: u16 = 0x980F;
/// Where the SPL loads the update loader (its loader.ld), and its largest size.
pub const LOADER_LOAD: u32 = 0x01C0_A800;
pub const LOADER_MAX: usize = 0x14000;
/// How Felucca, sloop and melodee (and SIDOU's ota.c) recognise a package's loader: in the
/// first literals of ota.bin's first LZ4 block.
pub const LOADER_MARK: &[u8; 16] = b"FELUCCA-LOADER-1";
/// Felucca 1.6.1's update loader, the RAM image its tools/build.py `build_loader()` makes from
/// the tag `v1.6.1` (commit a53ecf42), which SIDOU's fm1 build makes byte for byte from the
/// vendored copy (platform/fm1/VENDORED.md): its SHA-256. The loader in Felucca's own
/// felucca-1.6.1.fwsc unwraps to the same bytes. It refuses a flash.bin of any size but
/// `FLASH_SIZE` (-14) before it erases anything.
pub const FELUCCA_LOADER_SHA256: &str =
    "2cde42b729d787a3e591ed4eaaffbbf33cebee7d58421d33bf6b6d27cc03f1aa";
/// The release `FELUCCA_LOADER_SHA256` is the loader of.
pub const FELUCCA_LOADER_RELEASE: &str = "Felucca 1.6.1";
/// What that loader answers the update protocol's handshake with (its loader.c `FELUCCA_ID`).
pub const FELUCCA_LOADER_ID: &str = "ota-FM-1_900";
/// ota.c stages at most 0x5000 - 0x200 bytes of ota.bin's body.
pub const OTA_BODY_MAX: usize = 0x5000 - 0x200;
/// The identity of development builds, as every community firmware's.
pub const DEFAULT_PRODUCT: &str = "FM-1_900";

/// The file name the SPL looks for in the update record.
const LOADER_NAME: &[u8; 16] = b"usb_hid_ota.bin\0";
const UFW_CHIP: &[u8] = b"AC791N";
/// The flash header: JieLi SDK defaults (burner size 544, VID "0.01", flash size 0xFF000,
/// fs_ver 0x10, PID "AC791N_STORY"), plain, without its CRC (fm1pkg_make.py FLASH_HDR).
const FLASH_HDR: &str = "20022f104cc900f00f001010a4ff4e5d0b41e0cd781dc856417fb367cf9f";
/// After the key blob in isd_config.ini, as Felucca's packages carry it.
const ISD_TAIL: &[u8] = b"[FELUCCA]\r\n";
/// Where the SPL goes in the head.
const SPL_OFF: usize = 0xA0;
const MARK_BLOCKS: usize = 20;
const MARK_KEEP: usize = 0x2F;
const MARK_UNUSED: u8 = 0x7D;

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error(msg.into()))
}

/// CRC-16/XMODEM (poly 0x1021), from `init`.
pub fn crc16(data: &[u8], init: u16) -> u16 {
    let mut crc = init;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// JieLi's ENC stream cipher, in place: XOR with a CRC-16 LFSR's low byte. Its own inverse.
pub fn enc(buf: &mut [u8], mut key: u16) {
    for b in buf {
        *b ^= key as u8;
        key = (key << 1) ^ if key & 0x8000 != 0 { 0x1021 } else { 0 };
    }
}

/// The SFC cipher: ENC per 32-byte block, the key mixed with the block's offset.
pub fn sfc(buf: &mut [u8], key: u16) {
    for (i, block) in buf.chunks_mut(32).enumerate() {
        enc(block, key ^ ((i * 32) >> 2) as u16);
    }
}

/// The 16-bit chip key from isd_config.ini's 32-byte blob.
pub fn chip_key(blob: &[u8]) -> u16 {
    let sum = blob[..16].iter().map(|&b| u32::from(b)).sum::<u32>() & 0xFF;
    let sum = if sum >= 0xE0 {
        0xAA
    } else if sum <= 0x10 {
        0x55
    } else {
        sum
    };
    (0..16)
        .filter(|&i| u32::from(blob[16 + i] ^ blob[15 - i]) < sum)
        .fold(0u16, |k, i| k | 1 << i)
}

/// A JLFS entry's 16-byte name field.
enum Name<'a> {
    /// The name, a NUL, then 0xFF.
    Text(&'a str),
    /// 16 raw bytes: a name and the SDK packer's trailing fields.
    Raw([u8; 16]),
}

/// A 32-byte JLFS entry: its CRC-16, then data CRC, offset, size, flags, reserved, index, name.
fn entry(
    dcrc: u16,
    offset: u32,
    size: u32,
    flags: u8,
    resvd: u8,
    index: u16,
    name: Name,
) -> [u8; 32] {
    let mut e = [0u8; 32];
    e[2..4].copy_from_slice(&dcrc.to_le_bytes());
    e[4..8].copy_from_slice(&offset.to_le_bytes());
    e[8..12].copy_from_slice(&size.to_le_bytes());
    e[12] = flags;
    e[13] = resvd;
    e[14..16].copy_from_slice(&index.to_le_bytes());
    match name {
        Name::Text(s) => {
            let n = s.as_bytes();
            assert!(n.len() < 16);
            e[16..32].fill(0xFF);
            e[16..16 + n.len()].copy_from_slice(n);
            e[16 + n.len()] = 0;
        }
        Name::Raw(raw) => e[16..32].copy_from_slice(&raw),
    }
    let crc = crc16(&e[2..], 0);
    e[0..2].copy_from_slice(&crc.to_le_bytes());
    e
}

fn hex16(s: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex");
    }
    out
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).expect("fits 32 bits")
}

/// The inputs that are not SIDOU's: three files of JieLi's AC79 SDK (Apache-2.0).
pub struct Inputs {
    /// `cpu/wl82/tools/uboot.boot`: the SPL.
    pub uboot: Vec<u8>,
    /// The AC79 SDK's `cpu/wl82/tools/cfg_tool.bin`.
    pub cfg_tool: Vec<u8>,
    /// The AC79 SDK's `cpu/wl82/tools/cfg/eq_cfg_hw.bin`.
    pub eq_cfg_hw: Vec<u8>,
}

/// 32 bytes that `chip_key` turns into `key`, then their CRC-16 (fm1pkg_make.py `key_blob`).
pub fn key_blob(key: u16) -> [u8; 34] {
    let mut d = [0u8; 34];
    d[..16].fill(0x10); // sum 0 (mod 256): the threshold 0x55
    for i in 0..16 {
        d[16 + i] = d[15 - i] ^ if key >> i & 1 != 0 { 0x00 } else { 0xFF };
    }
    let crc = crc16(&d[..32], 0);
    d[32..].copy_from_slice(&crc.to_le_bytes());
    d
}

/// The head (fm1pkg_make.py `flash_image`'s first part): the flash header and four JLFS
/// entries (each ENC 0xFFFF), the SPL at 0xA0, isd_config right after it.
pub fn head(uboot: &[u8], key: u16) -> Result<Vec<u8>> {
    let mut isd = key_blob(key).to_vec();
    isd.extend_from_slice(ISD_TAIL);
    let isd_off = SPL_OFF + uboot.len();
    if isd_off + isd.len() > HEAD_SIZE {
        return err("the SPL does not fit in the head");
    }
    let mut h = vec![0xFFu8; HEAD_SIZE];
    h[SPL_OFF..isd_off].copy_from_slice(uboot);
    h[isd_off..isd_off + isd.len()].copy_from_slice(&isd);
    let fh = (0..FLASH_HDR.len() / 2)
        .map(|i| u8::from_str_radix(&FLASH_HDR[2 * i..2 * i + 2], 16).expect("hex"))
        .collect::<Vec<u8>>();
    let mut top = crc16(&fh, 0).to_le_bytes().to_vec();
    top.extend_from_slice(&fh);
    for e in [
        entry(
            crc16(uboot, 0),
            len32(SPL_OFF),
            len32(uboot.len()),
            0x00,
            0x00,
            0,
            Name::Text("uboot.boot"),
        ),
        entry(
            crc16(&isd, 0),
            len32(isd_off),
            len32(isd.len()),
            0x02,
            0x80,
            0,
            Name::Text("isd_config.ini"),
        ),
        entry(
            0xFFFF,
            0x4000,
            0xFFFF_FFFF,
            0x81,
            0xFF,
            0,
            Name::Text("app_dir_head"),
        ),
        entry(
            0xFFFF,
            0xFF000,
            0x1000,
            0x12,
            0x01,
            0xFFFF,
            Name::Text("key_mac"),
        ),
    ] {
        top.extend_from_slice(&e);
    }
    for block in top.chunks_mut(32) {
        enc(block, 0xFFFF);
    }
    h[..top.len()].copy_from_slice(&top);
    Ok(h)
}

/// flash.bin as Felucca's fm1pkg_make.py builds it: the head, then the app area.
pub fn flash_image(inputs: &Inputs, app: &[u8]) -> Result<Vec<u8>> {
    if app.len() > APP_SLOT {
        return err(format!(
            "app is {} bytes, the slot is {APP_SLOT}",
            app.len()
        ));
    }
    let key = CHIP_KEY;
    let flash_head = head(&inputs.uboot, key)?;
    let (cfg_tool, eq) = (&inputs.cfg_tool, &inputs.eq_cfg_hw);
    let mut slot = app.to_vec();
    slot.resize(APP_SLOT, 0xFF);
    let blk = 0x120 + APP_SLOT + cfg_tool.len();
    let mut area = vec![0xFFu8; blk];
    area[0x120..0x120 + APP_SLOT].copy_from_slice(&slot);
    area[0x120 + APP_SLOT..blk].copy_from_slice(cfg_tool);
    let ents = [
        entry(
            crc16(&slot, 0),
            0x120,
            len32(APP_SLOT),
            0x82,
            0xFF,
            0,
            Name::Text("app.bin"),
        ),
        entry(
            crc16(cfg_tool, 0),
            len32(0x120 + APP_SLOT),
            len32(cfg_tool.len()),
            0x82,
            0xFF,
            0,
            Name::Text("cfg_tool.bin"),
        ),
        // The region descriptors; the bytes after the names are SDK packer fields.
        entry(
            0xFFFF,
            0x93000,
            352_256,
            0x12,
            0x80,
            0,
            Name::Raw(hex16("564d00ffffffffffffffffff00800000")),
        ),
        entry(
            0xFFFF,
            0,
            len32(FLASH_SIZE),
            0x92,
            0x82,
            0,
            Name::Raw(hex16("5052435400ffffffffffffff434f4445")),
        ),
        entry(
            0xFFFF,
            0xE9000,
            0x1000,
            0x92,
            0x80,
            0,
            Name::Raw(hex16("4254494600ffffffffffffff4155544f")),
        ),
        entry(0xFFFF, 0xEA000, 0x12000, 0x92, 0x81, 1, Name::Text("USR")),
    ];
    for (i, e) in ents.iter().enumerate() {
        area[32 + 32 * i..64 + 32 * i].copy_from_slice(e);
    }
    let head = entry(
        crc16(&area[32..blk], 0),
        0x0200_0120,
        len32(blk),
        0x83,
        0xFF,
        0,
        Name::Text("app_area_head"),
    );
    area[..32].copy_from_slice(&head);
    let mut cfg_body = entry(
        crc16(eq, 0),
        0x40,
        len32(eq.len()),
        0x82,
        0xFF,
        1,
        Name::Text("eq_cfg_hw.bin"),
    )
    .to_vec();
    cfg_body.extend_from_slice(eq);
    let mut region = area;
    region.extend_from_slice(&entry(
        crc16(&cfg_body, 0),
        0x20,
        len32(0x20 + cfg_body.len()),
        0x83,
        0xFF,
        1,
        Name::Text("cfg"),
    ));
    region.extend_from_slice(&cfg_body);
    sfc(&mut region, key);
    if HEAD_SIZE + region.len() > FLASH_SIZE {
        return err("the app area overflows flash.bin");
    }
    let mut flash = vec![0xFFu8; FLASH_SIZE];
    flash[..HEAD_SIZE].copy_from_slice(&flash_head);
    flash[HEAD_SIZE..HEAD_SIZE + region.len()].copy_from_slice(&region);
    Ok(flash)
}

/// The chip key in a flash head: its JLFS top entries (ENC 0xFFFF) lead to isd_config.ini.
pub fn head_chip_key(head: &[u8]) -> Result<u16> {
    for o in (32..0x400).step_by(32) {
        let mut e = head[o..o + 32].to_vec();
        enc(&mut e, 0xFFFF);
        if crc16(&e[2..], 0) != u16::from_le_bytes([e[0], e[1]]) {
            return err(format!("the head's JLFS entry at {o:#x} is damaged"));
        }
        if e[16..31].starts_with(b"isd_config.ini") {
            let at = u32::from_le_bytes([e[4], e[5], e[6], e[7]]) as usize;
            let blob = head
                .get(at..at + 34)
                .ok_or(Error("isd_config outside the head".into()))?;
            if crc16(&blob[..32], 0) != u16::from_le_bytes([blob[32], blob[33]]) {
                return err("the head's isd_config blob is damaged");
            }
            return Ok(chip_key(blob));
        }
        if u16::from_le_bytes([e[14], e[15]]) != 0 {
            break;
        }
    }
    err("no isd_config.ini in the head")
}

fn ldr_head(dcrc: u16, a: u32, b: u32, attr: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    h[2..4].copy_from_slice(&dcrc.to_le_bytes());
    h[4..8].copy_from_slice(&a.to_le_bytes());
    h[8..12].copy_from_slice(&b.to_le_bytes());
    h[12] = attr;
    h[16..32].copy_from_slice(LOADER_NAME);
    let crc = crc16(&h[2..], 0);
    h[0..2].copy_from_slice(&crc.to_le_bytes());
    h
}

/// ota.bin from the loader's RAM image (Felucca's build.py `ldr_wrap`): an outer JLFS head,
/// an inner head with the load address, then `[clen u32][dlen u32][LZ4 block]` per 4096
/// bytes, compressed with Felucca's own compressor (lz4blk), so ota.bin is byte for byte the
/// one Felucca's packer makes from the same loader image.
pub fn wrap_loader(image: &[u8]) -> Result<Vec<u8>> {
    if image.is_empty() || image.len() > LOADER_MAX {
        return err(format!(
            "the loader is {} bytes (1..={LOADER_MAX})",
            image.len()
        ));
    }
    if !image[..image.len().min(64)]
        .windows(16)
        .any(|w| w == LOADER_MARK)
    {
        return err("the loader does not start with its marker (crt0_ldr.S)");
    }
    let mut blocks = Vec::new();
    for raw in image.chunks(4096) {
        let c = lz4blk::compress(raw);
        if c.len() > 4096 || lz4blk::decompress(&c).as_deref() != Some(raw) {
            return err("a loader block does not compress below 4096 bytes");
        }
        blocks.extend_from_slice(&len32(c.len()).to_le_bytes());
        blocks.extend_from_slice(&len32(raw.len()).to_le_bytes());
        blocks.extend_from_slice(&c);
    }
    let mut body = ldr_head(crc16(image, 0), len32(image.len()), LOADER_LOAD, 0).to_vec();
    body.extend_from_slice(&blocks);
    if body.len() > OTA_BODY_MAX {
        return err(format!(
            "ota.bin's body is {} bytes, ota.c stages {OTA_BODY_MAX}",
            body.len()
        ));
    }
    let mut ota = ldr_head(crc16(&body, 0), 0x20, len32(body.len()), 0x41).to_vec();
    ota.extend_from_slice(&body);
    Ok(ota)
}

/// The loader's RAM image back from ota.bin (for checks and tests).
pub fn unwrap_loader(ota: &[u8]) -> Result<Vec<u8>> {
    let inner = ota.get(32..64).ok_or(Error("ota.bin too short".into()))?;
    let size = u32::from_le_bytes([inner[4], inner[5], inner[6], inner[7]]) as usize;
    let (mut at, mut image) = (64, Vec::with_capacity(size));
    while image.len() < size {
        let word = |o: usize| -> Result<usize> {
            let b = ota.get(o..o + 4).ok_or(Error("truncated ota.bin".into()))?;
            Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        let (clen, dlen) = (word(at)?, word(at + 4)?);
        let c = ota
            .get(at + 8..at + 8 + clen)
            .ok_or(Error("truncated ota.bin".into()))?;
        let d = lz4blk::decompress(c).ok_or(Error("a damaged LZ4 block".into()))?;
        if d.len() != dlen {
            return err("an LZ4 block of the wrong size");
        }
        image.extend(d);
        at += 8 + clen;
    }
    Ok(image)
}

/// The `ufw` container with flash.bin (type 0) and ota.bin (type 100), the identity spelled by
/// marker bytes (Felucca's fm1pkg_make.py `ufw`).
pub fn ufw(flash: &[u8], ota: &[u8], product: &str) -> Result<Vec<u8>> {
    let p = product.as_bytes();
    if !product.is_ascii()
        || p.len() > MARK_BLOCKS
        || p.iter()
            .enumerate()
            .any(|(i, &c)| c.wrapping_add(i as u8 + 1) == MARK_UNUSED)
    {
        return err(format!(
            "product {product:?}: at most 20 ASCII characters, none encoding to the unused marker"
        ));
    }
    let files: [(u16, &str, &[u8]); 2] = [(0, "flash.bin", flash), (100, "ota.bin", ota)];
    let (mut off, mut ents) = (0x400usize, Vec::new());
    for (i, (typ, name, d)) in files.iter().enumerate() {
        off = (off + 0xFF) & !0xFF;
        let mut e = [0u8; 0x50];
        e[0..2].copy_from_slice(&typ.to_le_bytes());
        e[2..4].copy_from_slice(&(i as u16).to_le_bytes());
        e[4..6].copy_from_slice(&crc16(d, 0).to_le_bytes());
        e[8..12].copy_from_slice(&len32(off).to_le_bytes());
        e[12..16].copy_from_slice(&len32(d.len()).to_le_bytes());
        e[16..20].copy_from_slice(&len32((d.len() + 31) & !31).to_le_bytes());
        e[0x40..0x40 + name.len()].copy_from_slice(name.as_bytes());
        ents.push((e, off, *d));
        off += d.len();
    }
    let total = off;
    let mut hdr = [0u8; 0x40];
    hdr[4..8].copy_from_slice(&len32(total).to_le_bytes());
    hdr[8..10].copy_from_slice(&(files.len() as u16).to_le_bytes());
    hdr[10..12].copy_from_slice(&0x0004u16.to_le_bytes()); // as the SDK packer writes them
    hdr[12..14].copy_from_slice(&0x0200u16.to_le_bytes());
    hdr[16..16 + UFW_CHIP.len()].copy_from_slice(UFW_CHIP);
    let mut list = Vec::new();
    for (e, _, _) in &ents {
        let mut x = *e;
        enc(&mut x, 0xFFFF);
        list.extend_from_slice(&x);
    }
    let lcrc = crc16(&list, 0);
    hdr[2..4].copy_from_slice(&lcrc.to_le_bytes());
    let hcrc = crc16(&hdr[2..], 0);
    hdr[0..2].copy_from_slice(&hcrc.to_le_bytes());
    enc(&mut hdr, 0xFFFF);
    let mut logical = vec![0xFFu8; total];
    logical[..0x40].copy_from_slice(&hdr);
    logical[0x40..0x40 + list.len()].copy_from_slice(&list);
    for (_, o, d) in &ents {
        logical[*o..*o + d.len()].copy_from_slice(d);
    }
    let mut raw = Vec::with_capacity(total + MARK_BLOCKS);
    for i in 0..MARK_BLOCKS {
        raw.extend_from_slice(&logical[i * MARK_KEEP..(i + 1) * MARK_KEEP]);
        raw.push(
            p.get(i)
                .map_or(MARK_UNUSED, |&c| c.wrapping_add(i as u8 + 1)),
        );
    }
    raw.extend_from_slice(&logical[MARK_BLOCKS * MARK_KEEP..]);
    Ok(raw)
}

/// A package's identity, from its marker bytes.
pub fn product_of(package: &[u8]) -> String {
    (0..MARK_BLOCKS)
        .filter_map(|i| {
            let m = *package.get(i * (MARK_KEEP + 1) + MARK_KEEP)?;
            (m != MARK_UNUSED).then(|| char::from(m.wrapping_sub(i as u8 + 1)))
        })
        .collect()
}

/// The package without its marker bytes: what the device reads during an update.
pub fn logical_image(package: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(package.len());
    for i in 0..MARK_BLOCKS {
        out.extend_from_slice(&package[i * (MARK_KEEP + 1)..i * (MARK_KEEP + 1) + MARK_KEEP]);
    }
    out.extend_from_slice(&package[MARK_BLOCKS * (MARK_KEEP + 1)..]);
    out
}

/// A file in a package's `ufw` container by type: (offset in the logical image, bytes).
pub fn ufw_file(package: &[u8], typ: u16) -> Result<(usize, Vec<u8>)> {
    let logical = logical_image(package);
    let mut hdr = logical[..0x40].to_vec();
    enc(&mut hdr, 0xFFFF);
    if crc16(&hdr[2..], 0) != u16::from_le_bytes([hdr[0], hdr[1]]) {
        return err("not an FM-1 update package (header CRC)");
    }
    let n = u16::from_le_bytes([hdr[8], hdr[9]]) as usize;
    for i in 0..n {
        let at = 0x40 + i * 0x50;
        let mut e = logical
            .get(at..at + 0x50)
            .ok_or(Error("truncated entry list".into()))?
            .to_vec();
        enc(&mut e, 0xFFFF);
        if u16::from_le_bytes([e[0], e[1]]) == typ {
            let off = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
            let len = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
            let d = logical
                .get(off..off + len)
                .ok_or(Error("truncated package".into()))?;
            return Ok((off, d.to_vec()));
        }
    }
    err(format!("no file of type {typ} in the package"))
}

/// The data CRC16 in a package's entry for a file type.
fn ufw_entry_crc(package: &[u8], typ: u16) -> Option<u16> {
    let logical = logical_image(package);
    let mut hdr = logical[..0x40].to_vec();
    enc(&mut hdr, 0xFFFF);
    for i in 0..u16::from_le_bytes([hdr[8], hdr[9]]) as usize {
        let at = 0x40 + i * 0x50;
        let mut e = logical.get(at..at + 0x50)?.to_vec();
        enc(&mut e, 0xFFFF);
        if u16::from_le_bytes([e[0], e[1]]) == typ {
            return Some(u16::from_le_bytes([e[4], e[5]]));
        }
    }
    None
}

/// Checks that the app answers the update protocol with `product` (the identity string is in
/// its image, NUL-terminated): an installer verifies the identity after the install.
pub fn check_app_identity(app: &[u8], product: &str) -> Result<()> {
    let mut want = product.as_bytes().to_vec();
    want.push(0);
    if app.windows(want.len()).any(|w| w == want) {
        Ok(())
    } else {
        err(format!(
            "the app does not carry the identity {product:?}: build it with SIDOU_PRODUCT={product}"
        ))
    }
}

/// The whole package from the firmware image and the loader's RAM image.
pub fn package(inputs: &Inputs, app: &[u8], loader: &[u8], product: &str) -> Result<Vec<u8>> {
    check_app_identity(app, product)?;
    ufw(&flash_image(inputs, app)?, &wrap_loader(loader)?, product)
}

/// What `verify` found in a package.
#[derive(Debug, Default)]
pub struct Verified {
    pub sha256: String,
    pub size: usize,
    pub identity: String,
    /// flash.bin's length (`FLASH_SIZE` for this board).
    pub flash_len: usize,
    /// The firmware image's length in the app slot (its trailing 0xFF padding trimmed).
    pub app_len: usize,
    /// The update loader's RAM image, unwrapped from ota.bin.
    pub loader_len: usize,
    /// The unwrapped loader's SHA-256.
    pub loader_sha256: String,
    /// The handshake identity in the loader's image (the first NUL-terminated `ota-…`
    /// string), empty if it has none.
    pub loader_identity: String,
    pub marker: bool,
    /// Every check that failed; empty when the package is sound.
    pub problems: Vec<String>,
}

/// An identity a SIDOU package may answer with: the development build's or a release's,
/// `FM-1_9XY`.
pub fn identity_ok(id: &str) -> bool {
    id.len() == 8 && id.starts_with("FM-1_9") && id[6..].bytes().all(|b| b.is_ascii_digit())
}

/// Checks a finished package without any input file or device: the container's CRCs and the
/// two files' CRCs, the identity (and that the app carries it), flash.bin's size (the size a
/// loader of this board accepts), the head's chip key, the app area's own CRCs, and the update
/// loader's unwrapped image (size, load marker). The problems are collected, not just the
/// first.
pub fn verify(package: &[u8]) -> Verified {
    let mut v = Verified {
        sha256: sha256_hex(package),
        size: package.len(),
        identity: String::new(),
        ..Verified::default()
    };
    if package.len() < MARK_BLOCKS * (MARK_KEEP + 1) + 0x400 {
        v.problems.push(format!(
            "{} bytes is too short for an FM-1 update package",
            package.len()
        ));
        return v;
    }
    v.identity = product_of(package);
    if !identity_ok(&v.identity) {
        v.problems.push(format!(
            "identity {:?} is neither FM-1_900 nor a release FM-1_9XY",
            v.identity
        ));
    }
    let (flash, ota) = match (ufw_file(package, 0), ufw_file(package, 100)) {
        (Ok((_, f)), Ok((_, o))) => (f, o),
        (f, o) => {
            for e in [f.err(), o.err()].into_iter().flatten() {
                v.problems.push(e.0);
            }
            return v;
        }
    };
    v.flash_len = flash.len();
    for (name, typ, data) in [("flash.bin", 0, &flash), ("ota.bin", 100, &ota)] {
        if ufw_entry_crc(package, typ) != Some(crc16(data, 0)) {
            v.problems
                .push(format!("{name} fails the CRC16 of its package entry"));
        }
    }
    if flash.len() != FLASH_SIZE {
        v.problems.push(format!(
            "flash.bin is {} bytes, this board's is {FLASH_SIZE}: the loader refuses it (-14)",
            flash.len()
        ));
    } else {
        verify_flash(&flash, &mut v);
    }
    match unwrap_loader(&ota) {
        Ok(image) => {
            v.loader_len = image.len();
            v.loader_sha256 = sha256_hex(&image);
            v.loader_identity = loader_identity(&image);
            v.marker = image.len() >= 64 && image[..64].windows(16).any(|w| w == LOADER_MARK);
            if image.is_empty() || image.len() > LOADER_MAX {
                v.problems.push(format!(
                    "the loader is {} bytes (1..={LOADER_MAX})",
                    image.len()
                ));
            }
            if !v.marker {
                v.problems.push(format!(
                    "the loader lacks the {} marker",
                    String::from_utf8_lossy(LOADER_MARK)
                ));
            }
        }
        Err(e) => v.problems.push(format!("ota.bin: {}", e.0)),
    }
    v
}

/// The first NUL-terminated printable string starting `ota-` in a loader image: what the loader
/// answers an installer's handshake with. Empty if there is none.
pub fn loader_identity(image: &[u8]) -> String {
    let mut from = 0;
    while let Some(at) = image[from..].windows(4).position(|w| w == b"ota-") {
        let start = from + at;
        let rest = &image[start..];
        if let Some(end) = rest.iter().take(32).position(|&b| b == 0) {
            if rest[..end].iter().all(|b| b.is_ascii_graphic()) {
                return String::from_utf8_lossy(&rest[..end]).into_owned();
            }
        }
        from = start + 4;
    }
    String::new()
}

/// The release a loader image is, by its SHA-256: `FELUCCA_LOADER_RELEASE` or None.
pub fn loader_release(sha256: &str) -> Option<&'static str> {
    (sha256 == FELUCCA_LOADER_SHA256).then_some(FELUCCA_LOADER_RELEASE)
}

fn verify_flash(flash: &[u8], v: &mut Verified) {
    match head_chip_key(&flash[..HEAD_SIZE]) {
        Ok(CHIP_KEY) => {}
        Ok(k) => v.problems.push(format!(
            "the head's chip key is {k:#06x}, not {CHIP_KEY:#06x}"
        )),
        Err(e) => v.problems.push(e.0),
    }
    let mut region = flash[HEAD_SIZE..].to_vec();
    sfc(&mut region, CHIP_KEY);
    let u16_at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let u32_at = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let head = &region[..32];
    if crc16(&head[2..], 0) != u16_at(head, 0) {
        v.problems
            .push("the app area's head entry fails its CRC (wrong key or damaged)".into());
        return;
    }
    let blk = u32_at(head, 8) as usize;
    match region.get(32..blk) {
        Some(body) if crc16(body, 0) == u16_at(head, 2) => {}
        _ => v.problems.push("the app area fails its CRC".into()),
    }
    let slot = region.get(0x120..0x120 + APP_SLOT);
    let Some(slot) = slot else {
        v.problems.push("the app slot is missing".into());
        return;
    };
    let app_entry = &region[32..64];
    if crc16(&app_entry[2..], 0) != u16_at(app_entry, 0) || crc16(slot, 0) != u16_at(app_entry, 2) {
        v.problems.push("app.bin fails its CRC".into());
    }
    let end = slot.iter().rposition(|&b| b != 0xFF).map_or(0, |i| i + 1);
    v.app_len = end;
    if let Err(e) = check_app_identity(&slot[..end], &v.identity) {
        v.problems.push(e.0);
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `inputs.toml`: the fetched files' URLs and SHA-256s.
pub struct Pins {
    pub ac79_base: String,
    /// (path under the SDK's tools directory and the cache, SHA-256)
    pub uboot: (String, String),
    pub cfg_tool: (String, String),
    pub eq_cfg_hw: (String, String),
}

impl Pins {
    pub fn get() -> Self {
        let text = include_str!("../inputs.toml");
        let val = |key: &str| -> String {
            text.lines()
                .find_map(|l| l.strip_prefix(key)?.trim_start().strip_prefix('='))
                .map(|v| v.trim().trim_matches('"').to_owned())
                .unwrap_or_else(|| panic!("inputs.toml: no {key}"))
        };
        let pair = |key: &str| {
            let v = val(key);
            let (p, s) = v.split_once(' ').expect("path sha256");
            (p.to_owned(), s.to_owned())
        };
        Pins {
            ac79_base: val("ac79_base"),
            uboot: pair("uboot"),
            cfg_tool: pair("cfg_tool"),
            eq_cfg_hw: pair("eq_cfg_hw"),
        }
    }
}

/// `path` if it holds the pinned bytes, else a fresh download from `url` (curl), checked.
pub fn fetched(path: &Path, url: &str, sha: &str, fetch: bool) -> Result<Vec<u8>> {
    if let Ok(d) = std::fs::read(path) {
        if sha256_hex(&d) == sha {
            return Ok(d);
        }
    }
    if !fetch {
        return err(format!(
            "{} is missing or not the pinned file",
            path.display()
        ));
    }
    let dir = path.parent().expect("a cache directory");
    std::fs::create_dir_all(dir).map_err(|e| Error(format!("{}: {e}", dir.display())))?;
    let part = path.with_extension("part");
    let ok = Command::new("curl")
        .args([
            "-sSfL",
            "--retry",
            "3",
            "--connect-timeout",
            "20",
            "--max-time",
            "300",
            "-o",
        ])
        .arg(&part)
        .arg(url)
        .status()
        .map_err(|e| Error(format!("curl: {e}")))?
        .success();
    if !ok {
        return err(format!("could not download {url}"));
    }
    let d = std::fs::read(&part).map_err(|e| Error(format!("{}: {e}", part.display())))?;
    if sha256_hex(&d) != sha {
        let _ = std::fs::remove_file(&part);
        return err(format!(
            "{url}: SHA-256 {} is not the pinned {sha}",
            sha256_hex(&d)
        ));
    }
    std::fs::rename(&part, path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
    Ok(d)
}

/// The repository's `build/` directory, the default cache.
pub fn default_cache() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../build")
}

/// The inputs from the cache (`build/ac79/`), downloading what is missing when `fetch` is set.
pub fn inputs(cache: &Path, fetch: bool) -> Result<Inputs> {
    let pins = Pins::get();
    let sdk = |(rel, sha): &(String, String)| {
        fetched(
            &cache.join("ac79").join(rel),
            &format!("{}{rel}", pins.ac79_base),
            sha,
            fetch,
        )
    };
    Ok(Inputs {
        uboot: sdk(&pins.uboot)?,
        cfg_tool: sdk(&pins.cfg_tool)?,
        eq_cfg_hw: sdk(&pins.eq_cfg_hw)?,
    })
}

/// The ota.c harness (harness/ota_harness.c): Felucca's ota.c, SIDOU's step 1 of the update
/// protocol, built for the host, so the tests can run it against packages from this crate; and
/// the flash spike's bounds (harness/flash_spike_guard.c).
#[doc(hidden)]
pub mod harness {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct OtaHost {
        pub ctx: *mut c_void,
        pub send: unsafe extern "C" fn(*mut c_void, *const u8, u32) -> i32,
        pub frame: unsafe extern "C" fn(*mut c_void, *mut *const u8, *mut u32) -> i32,
        pub frame_done: unsafe extern "C" fn(*mut c_void),
        pub flash: *mut u8,
        pub flash_len: u32,
        pub now: u32,
        pub committed: i32,
        pub parm: [u8; 112],
        pub shown_step: u32,
        pub shown_code: i32,
    }

    /// harness/flash_hooks.c's counters.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FhCounters {
        pub refused: u32,
        pub leds_dark: u32,
        pub unmasked_driver_calls: u32,
        pub read_windows: u32,
        pub oversized_windows: u32,
        pub erases: u32,
        pub writes: u32,
        pub area: u32,
        pub area_len: u32,
    }

    /// harness/flash_spike_guard.c's counters and the spike's bounds.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FsCounters {
        pub calls: u32,
        pub lo: u32,
        pub hi: u32,
        pub refused: u32,
        pub spike_lo: u32,
        pub spike_hi: u32,
        pub pattern: u32,
        pub measure: u32,
        pub sector_use: u32,
        pub store_lo: u32,
        pub store_hi: u32,
    }

    extern "C" {
        pub fn fs_reset(fill: u8);
        pub fn fs_flash() -> *mut u8;
        pub fn fs_counts() -> FsCounters;
        pub fn fs_erase(off: u32) -> i32;
        pub fn fs_page_erase(off: u32) -> i32;
        pub fn fs_prog(off: u32, p: *const u8, n: u32) -> i32;
        pub fn fs_suspend(erase_off: u32, read_off: u32, dst: *mut u8, n: u32) -> i32;
        pub fn fs_driver_ok(off: u32, n: u32) -> i32;
        pub fn fs_classify(sector: *const u8) -> i32;
        pub fn fs_pattern_state(sector: *const u8) -> i32;
        pub fn fs_pattern_byte(i: u32) -> u8;
        pub fn fs_mark() -> *const std::ffi::c_char;
        pub fn fh_reset(audio: i32);
        pub fn fh_flash() -> *mut u8;
        pub fn fh_counts() -> FhCounters;
        pub fn fh_erase(off: u32) -> i32;
        pub fn fh_prog(off: u32, p: *const u8, n: u32) -> i32;
        pub fn fh_fread(off: u32, p: *mut u8, n: u32) -> i32;
        pub fn ota_h_service(h: *mut OtaHost);
        pub fn ota_h_session(h: *mut OtaHost) -> i32;
        pub fn ota_h_boot_cleanup(h: *mut OtaHost) -> i32;
        pub fn ldr_h_session(h: *mut OtaHost) -> i32;
        pub fn ldr_h_writes() -> u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_is_xmodem() {
        assert_eq!(crc16(b"123456789", 0), 0x31C3);
        assert_eq!(crc16(b"", 0), 0);
    }

    #[test]
    fn enc_and_sfc_are_their_own_inverse() {
        let plain: Vec<u8> = (0..200u8).collect();
        let mut x = plain.clone();
        enc(&mut x, 0xFFFF);
        assert_ne!(x, plain);
        enc(&mut x, 0xFFFF);
        assert_eq!(x, plain);
        sfc(&mut x, CHIP_KEY);
        assert_ne!(x, plain);
        sfc(&mut x, CHIP_KEY);
        assert_eq!(x, plain);
    }

    #[test]
    fn the_head_carries_the_chip_key() {
        assert_eq!(chip_key(&key_blob(CHIP_KEY)), CHIP_KEY);
        let h = head(&[0x5A; 1000], CHIP_KEY).unwrap();
        assert_eq!(head_chip_key(&h).unwrap(), CHIP_KEY);
        assert!(
            head(&[0; 0x4000], CHIP_KEY).is_err(),
            "an SPL too big for the head"
        );
    }

    #[test]
    fn entries_carry_their_crc() {
        let e = entry(0x1234, 0x120, 0x8DFBC, 0x82, 0xFF, 0, Name::Text("app.bin"));
        assert_eq!(u16::from_le_bytes([e[0], e[1]]), crc16(&e[2..], 0));
        assert_eq!(&e[16..24], b"app.bin\0");
        assert!(e[24..32].iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn the_loader_wraps_and_unwraps() {
        let mut image = b"\x04\x88FELUCCA-LOADER-1".to_vec();
        image.extend((0..9000u32).map(|i| (i * 7 % 251) as u8));
        let ota = wrap_loader(&image).unwrap();
        assert_eq!(unwrap_loader(&ota).unwrap(), image);
        assert_eq!(&ota[16..32], LOADER_NAME);
        assert_eq!(ota[12], 0x41);
        assert_eq!(
            u32::from_le_bytes(ota[40..44].try_into().unwrap()),
            LOADER_LOAD
        );
        // ota.c finds the marker among the first LZ4 literals: bytes 0x48..0x88.
        assert!(ota[0x48..0x48 + 80].windows(16).any(|w| w == LOADER_MARK));
        assert!(wrap_loader(&image[2..]).is_err() || image[2..].starts_with(LOADER_MARK));
        assert!(wrap_loader(&[0u8; 100]).is_err(), "no marker");
    }

    #[test]
    fn the_identity_round_trips_through_the_markers() {
        let raw = ufw(&[1, 2, 3], &[4, 5], "FM-1_900").unwrap();
        assert_eq!(product_of(&raw), "FM-1_900");
        assert_eq!(ufw_file(&raw, 0).unwrap().1, [1, 2, 3]);
        assert_eq!(ufw_file(&raw, 100).unwrap().1, [4, 5]);
        assert!(ufw(&[], &[], "this identity is far too long").is_err());
    }

    #[test]
    fn the_app_must_carry_the_identity() {
        assert!(check_app_identity(b"..FM-1_900\0..", "FM-1_900").is_ok());
        assert!(check_app_identity(b"..FM-1_9001\0..", "FM-1_900").is_err());
    }
}
