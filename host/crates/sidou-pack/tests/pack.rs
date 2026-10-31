// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sidou_pack::{
    default_cache, flash_image, head, head_chip_key, inputs, loader_identity, loader_release,
    package, product_of, sha256_hex, ufw, ufw_file, unwrap_loader, verify, wrap_loader, Inputs,
    APP_SLOT, CHIP_KEY, DEFAULT_OPTIONS, DEFAULT_PRODUCT, FLASH_SIZE, HEAD_SIZE, LOADER_LOAD,
};

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

fn tiny_app_built_with(change: &[(&str, &str)]) -> Vec<u8> {
    let mut a = tiny_app();
    a.extend_from_slice(b"SIDOU-OPTIONS 1");
    for (k, v) in DEFAULT_OPTIONS {
        let v = change.iter().find(|(n, _)| *n == k).map_or(v, |c| c.1);
        a.extend_from_slice(format!(" {k}={v}").as_bytes());
    }
    a.extend_from_slice(b" SIDOU_PRODUCT=FM-1_900\0");
    a
}

fn tiny_release_app() -> Vec<u8> {
    tiny_app_built_with(&[])
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
    assert_eq!(u.raw.len(), FLASH_SIZE);
    assert_eq!(
        &u.raw[..HEAD_SIZE],
        head(&inp.uboot, CHIP_KEY).unwrap().as_slice()
    );
    assert_eq!(head_chip_key(&u.raw[..HEAD_SIZE]).unwrap(), CHIP_KEY);
    assert_eq!(&u.raw[0xA0..0xA0 + inp.uboot.len()], inp.uboot.as_slice());
    assert_eq!(u.area, HEAD_SIZE);
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

#[test]
fn verify_accepts_a_sound_package_and_reports_its_facts() {
    let Some(inp) = cached() else {
        return;
    };
    let (app, loader) = (tiny_release_app(), tiny_loader());
    let pkg = package(&inp, &app, &loader, DEFAULT_PRODUCT).unwrap();
    let v = verify(&pkg);
    assert!(v.problems.is_empty(), "{:?}", v.problems);
    assert_eq!(v.sha256, sha256_hex(&pkg));
    assert_eq!(
        (v.identity.as_str(), v.flash_len, v.app_len, v.loader_len),
        (DEFAULT_PRODUCT, FLASH_SIZE, app.len(), loader.len())
    );
    assert!(v.marker);
    assert_eq!(v.loader_sha256, sha256_hex(&loader));
    assert_eq!(
        (v.loader_identity.as_str(), loader_release(&v.loader_sha256)),
        ("", None),
        "the tiny loader answers nothing and is no release"
    );
    let release = package(&inp, &app, &loader, "FM-1_921");
    assert!(
        release.is_err(),
        "the app must carry the identity it is packed with"
    );
}

#[test]
fn verify_refuses_an_app_built_with_other_options_than_the_defaults() {
    let Some(inp) = cached() else {
        return;
    };
    let pack = |app: &[u8]| package(&inp, app, &tiny_loader(), DEFAULT_PRODUCT).unwrap();
    let ok = verify(&pack(&tiny_release_app()));
    assert!(ok.problems.is_empty(), "{:?}", ok.problems);
    assert!(ok.options.contains("SIDOU_ALL_COMBINED_PROFILES_8580=ON"));
    let stale = tiny_app_built_with(&[("SIDOU_ALL_COMBINED_PROFILES_8580", "OFF")]);
    let v = verify(&pack(&stale));
    assert_eq!(v.problems.len(), 1, "{:?}", v.problems);
    assert!(v.problems[0].contains("SIDOU_ALL_COMBINED_PROFILES_8580=OFF"));
    assert!(v.options.contains("SIDOU_ALL_COMBINED_PROFILES_8580=OFF"));
    let v = verify(&pack(&tiny_app()));
    assert!(v.problems.iter().any(|p| p.contains("no build options")));
}

#[test]
fn verify_refuses_a_package_of_the_flash_spike() {
    let Some(inp) = cached() else {
        return;
    };
    let app = tiny_app_built_with(&[("SIDOU_FLASH_SPIKE", "ON")]);
    let v = verify(&package(&inp, &app, &tiny_loader(), DEFAULT_PRODUCT).unwrap());
    assert!(
        v.problems.len() == 1 && v.problems[0].contains("SIDOU_FLASH_SPIKE=ON"),
        "{:?}",
        v.problems
    );
}

#[test]
fn verify_refuses_a_wrong_size_package() {
    let Some(inp) = cached() else {
        return;
    };
    let mut flash = flash_image(&inp, &tiny_app()).unwrap();
    flash.truncate(flash.len() - 0x1000);
    let ota = wrap_loader(&tiny_loader()).unwrap();
    let pkg = ufw(&flash, &ota, DEFAULT_PRODUCT).unwrap();
    let v = verify(&pkg);
    assert!(
        v.problems.iter().any(|p| p.contains("(-14)")),
        "{:?}",
        v.problems
    );
}

#[test]
fn verify_refuses_damage_and_foreign_identities() {
    let Some(inp) = cached() else {
        return;
    };
    let pkg = package(&inp, &tiny_app(), &tiny_loader(), DEFAULT_PRODUCT).unwrap();
    let mut bad = pkg.clone();
    let at = 0x400 + HEAD_SIZE + 0x200;
    bad[at] ^= 1;
    assert!(!verify(&bad).problems.is_empty());
    assert!(!verify(&[0u8; 100]).problems.is_empty());
    assert!(!verify(&vec![0u8; 5000]).problems.is_empty());
    let ota = wrap_loader(&tiny_loader()).unwrap();
    let mut flash = flash_image(&inp, &tiny_app()).unwrap();
    flash.truncate(FLASH_SIZE);
    let stock = ufw(&flash, &ota, "FM-1_015").unwrap();
    assert!(verify(&stock)
        .problems
        .iter()
        .any(|p| p.contains("FM-1_015")));
}

#[test]
fn the_loader_identity_is_its_first_terminated_ota_string() {
    assert_eq!(
        loader_identity(b"\x00ota-FM-1_900\x00ota-x\x00"),
        "ota-FM-1_900"
    );
    let mut l = b"ota-".to_vec();
    l.extend([b'a'; 40]);
    l.extend_from_slice(b"\x00ota-\x01\x00ota-FM-1_015\x00");
    assert_eq!(loader_identity(&l), "ota-FM-1_015");
    assert_eq!(loader_identity(b"no identity here\x00"), "");
}
