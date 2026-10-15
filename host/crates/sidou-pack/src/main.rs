// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `sidou-pack --app sidou_app.bin --loader loader/loader.bin -o sidou.fwsc`
//!
//! Packs the firmware image and the update loader's RAM image (both from the fm1 build) into
//! an FM-1 update package. Downloads the inputs that are not SIDOU's on first use into the
//! cache (default: the repository's `build/`), checked against `inputs.toml`.

use std::path::PathBuf;
use std::process::ExitCode;

use sidou_pack::{default_cache, inputs, package, product_of, sha256_hex, DEFAULT_PRODUCT};

const USAGE: &str = "usage: sidou-pack --app APP.bin --loader LOADER.bin -o OUT.fwsc \
                     [--product FM-1_900] [--cache DIR] [--offline] [--ota OUT_OTA.bin] \
                     [--test-wrong-size]";

struct Args {
    app: PathBuf,
    loader: PathBuf,
    out: PathBuf,
    product: String,
    cache: PathBuf,
    fetch: bool,
    ota: Option<PathBuf>,
    /// docs/hardware-test.md: a package whose flash.bin is 4 KiB short, which the firmware's
    /// step 1 refuses before it erases anything.
    wrong_size: bool,
}

fn parse() -> Result<Args, String> {
    let (mut app, mut loader, mut out, mut ota) = (None, None, None, None);
    let (mut product, mut cache, mut fetch) = (DEFAULT_PRODUCT.to_owned(), default_cache(), true);
    let mut wrong_size = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--app" => app = Some(PathBuf::from(val()?)),
            "--loader" => loader = Some(PathBuf::from(val()?)),
            "-o" | "--out" => out = Some(PathBuf::from(val()?)),
            "--product" => product = val()?,
            "--cache" => cache = PathBuf::from(val()?),
            "--ota" => ota = Some(PathBuf::from(val()?)),
            "--offline" => fetch = false,
            "--test-wrong-size" => wrong_size = true,
            "-h" | "--help" => return Err(USAGE.into()),
            _ => return Err(format!("unknown argument {a}\n{USAGE}")),
        }
    }
    Ok(Args {
        app: app.ok_or(USAGE)?,
        loader: loader.ok_or(USAGE)?,
        out: out.ok_or(USAGE)?,
        product,
        cache,
        fetch,
        ota,
        wrong_size,
    })
}

fn run(a: &Args) -> Result<(), String> {
    let read = |p: &PathBuf| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let (app, loader) = (read(&a.app)?, read(&a.loader)?);
    let inputs = inputs(&a.cache, a.fetch).map_err(|e| e.to_string())?;
    let pkg = if a.wrong_size {
        sidou_pack::check_app_identity(&app, &a.product).map_err(|e| e.to_string())?;
        let mut flash = sidou_pack::flash_image(&inputs, &app).map_err(|e| e.to_string())?;
        flash.truncate(flash.len() - 0x1000);
        let ota = sidou_pack::wrap_loader(&loader).map_err(|e| e.to_string())?;
        sidou_pack::ufw(&flash, &ota, &a.product).map_err(|e| e.to_string())?
    } else {
        package(&inputs, &app, &loader, &a.product).map_err(|e| e.to_string())?
    };
    std::fs::write(&a.out, &pkg).map_err(|e| format!("{}: {e}", a.out.display()))?;
    if let Some(p) = &a.ota {
        let ota = sidou_pack::wrap_loader(&loader).map_err(|e| e.to_string())?;
        std::fs::write(p, ota).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    println!(
        "{}: {} bytes, identity {}, app {} bytes, loader {} bytes, sha256 {}",
        a.out.display(),
        pkg.len(),
        product_of(&pkg),
        app.len(),
        loader.len(),
        sha256_hex(&pkg)
    );
    Ok(())
}

fn main() -> ExitCode {
    match parse().and_then(|a| run(&a)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sidou-pack: {e}");
            ExitCode::FAILURE
        }
    }
}
