// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::process::ExitCode;

use sidou_pack::{default_cache, inputs, package, product_of, sha256_hex, DEFAULT_PRODUCT};

const USAGE: &str = "usage: sidou-pack --app APP.bin --loader LOADER.bin -o OUT.fwsc \
                     [--product FM-1_900] [--cache DIR] [--offline] [--ota OUT_OTA.bin] \
                     [--test-wrong-size]\n       sidou-pack --verify PACKAGE.fwsc";

struct Args {
    app: PathBuf,
    loader: PathBuf,
    out: PathBuf,
    product: String,
    cache: PathBuf,
    fetch: bool,
    ota: Option<PathBuf>,
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
    match sidou_pack::build_options(&app) {
        Some(o) => {
            let p = sidou_pack::option_problems(&o);
            if !p.is_empty() {
                eprintln!(
                    "sidou-pack: NOT A RELEASE BUILD, --verify and tools/preflash will fail it:"
                );
                for line in p {
                    eprintln!("  {line}");
                }
            }
        }
        None => eprintln!("sidou-pack: the app records no build options; --verify will fail it"),
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

fn verify_main(path: &str) -> ExitCode {
    let pkg = match std::fs::read(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sidou-pack: {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let v = sidou_pack::verify(&pkg);
    println!("sha256 {}", v.sha256);
    println!("size {}", v.size);
    println!("identity {}", v.identity);
    println!("flash_bin {}", v.flash_len);
    println!("app {}", v.app_len);
    println!("loader {}", v.loader_len);
    println!("loader_sha256 {}", v.loader_sha256);
    println!("loader_identity {}", v.loader_identity);
    println!(
        "loader_release {}",
        sidou_pack::loader_release(&v.loader_sha256).unwrap_or("unknown")
    );
    println!("options {}", v.options);
    println!(
        "marker {}",
        if v.marker {
            "FELUCCA-LOADER-1"
        } else {
            "missing"
        }
    );
    for p in &v.problems {
        println!("problem {p}");
    }
    if v.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("--verify") {
        return match argv.as_slice() {
            [_, path] => verify_main(path),
            _ => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        };
    }
    match parse().and_then(|a| run(&a)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sidou-pack: {e}");
            ExitCode::FAILURE
        }
    }
}
