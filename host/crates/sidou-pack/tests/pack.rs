// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Packages with the real inputs: read back with fm1-emu's reader (`fm1_soc::fwsc`), and a
//! golden hash. They need the cache (`build/ac79/`, which `sidou-pack` fills on first use)
//! and skip without it.

use sidou_pack::{
    default_cache, head, head_chip_key, inputs, package, product_of, sha256_hex, ufw_file,
    unwrap_loader, Inputs, APP_SLOT, CHIP_KEY, DEFAULT_PRODUCT, FLASH_SIZE, HEAD_SIZE, LOADER_LOAD,
};

/// The package of `tiny_app` and `tiny_loader` with the pinned inputs.
/// Checked once against Felucca's fm1pkg_make.py on the same inputs: byte-identical.
const GOLDEN_SHA256: &str = "03fbd6443eff4b337a8d737f3286aa297264cdf33fc9c6d3681fd82d21b1bafb";

fn cached() -> Option<Inputs> {
    match inputs(&default_cache(), false) {
        Ok(i) => Some(i),
        Err(e) => {
            println!("skipping: {e}; run sidou-pack once (it downloads the inputs into build/)");
            None
        }
    }
}

fn tiny_app() -> Vec<u8> {
    let mut a = b"SIDOU golden app ".to_vec();
    a.extend_from_slice(b"FM-1_900\0");
    a.extend((0..4096u32).map(|i| (i * 7 + 3) as u8));
    a
}

fn tiny_loader() -> Vec<u8> {
    let mut l = b"\x04\x88FELUCCA-LOADER-1".to_vec();
    l.extend((0..3000u32).map(|i| (i * 11 % 241) as u8));
    l
}

#[test]
fn a_package_reads_back_with_fm1_emus_reader() {
    let Some(inp) = cached() else {
        return;
    };
    let (app, loader) = (tiny_app(), tiny_loader());
    let pkg = package(&inp, &app, &loader, DEFAULT_PRODUCT).unwrap();
    let u = fm1_soc::fwsc::unpack(&pkg).expect("fm1-emu reads the package");
    // flash.bin: the SDK-default head with the SDK's SPL and the chip key, then the app area.
    assert_eq!(u.raw.len(), FLASH_SIZE);
    assert_eq!(
        &u.raw[..HEAD_SIZE],
        head(&inp.uboot, CHIP_KEY).unwrap().as_slice()
    );
    assert_eq!(head_chip_key(&u.raw[..HEAD_SIZE]).unwrap(), CHIP_KEY);
    assert_eq!(&u.raw[0xA0..0xA0 + inp.uboot.len()], inp.uboot.as_slice());
    assert_eq!(u.area, HEAD_SIZE);
    // The app area unscrambled: the directory and the app where the SPL maps it (0x02000120).
    let entry = u.entry("app.bin").expect("app.bin in the app area");
    assert_eq!((entry.offset, entry.size), (0x120, APP_SLOT));
    assert_eq!(&u.xip[0x120..0x120 + app.len()], app.as_slice());
    assert!(u.xip[0x120 + app.len()..0x120 + APP_SLOT]
        .iter()
        .all(|&b| b == 0xFF));
    for name in ["cfg_tool.bin", "USR"] {
        assert!(u.entry(name).is_some(), "no {name} in the app area");
    }
    let usr = u.entry("USR").unwrap();
    assert_eq!((usr.offset, usr.size), (0xEA000, 0x12000));
    // The ufw container: identity, flash.bin at 0x400, ota.bin after it.
    assert_eq!(product_of(&pkg), DEFAULT_PRODUCT);
    let (fl_off, flash) = ufw_file(&pkg, 0).unwrap();
    let (ota_off, ota) = ufw_file(&pkg, 100).unwrap();
    assert_eq!((fl_off, flash.len()), (0x400, FLASH_SIZE));
    assert_eq!(ota_off, (0x400 + FLASH_SIZE + 0xFF) & !0xFF);
    assert_eq!(unwrap_loader(&ota).unwrap(), loader);
    assert_eq!(
        u32::from_le_bytes(ota[40..44].try_into().unwrap()),
        LOADER_LOAD
    );
}

#[test]
fn the_golden_package() {
    let Some(inp) = cached() else {
        return;
    };
    let pkg = package(&inp, &tiny_app(), &tiny_loader(), DEFAULT_PRODUCT).unwrap();
    let sha = sha256_hex(&pkg);
    println!("golden package: {} bytes, sha256 {sha}", pkg.len());
    assert_eq!(
        sha, GOLDEN_SHA256,
        "the package format changed: check it, then update GOLDEN_SHA256"
    );
}
