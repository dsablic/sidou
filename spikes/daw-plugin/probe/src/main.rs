// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use fm1_plugin::engine::{self, Cmd, Midi, Shared, View, RATE};
use fm1_plugin::sidou::{self, Panel, StateMap};
use sidou_sys::ilp32;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const LCD_SIDE: usize = 240;
const FRAMES_PER_S: u64 = RATE as u64;
const LEAD: usize = 2048;
const ENV_BUTTON: u8 = sidou_sys::PANEL_BTN_ENV as u8;

struct Rig {
    shared: Arc<Shared>,
    cmds: mpsc::Sender<Cmd>,
    midi: rtrb::Producer<Midi>,
    audio: rtrb::Consumer<[f32; 2]>,
    thread: Option<std::thread::JoinHandle<()>>,
    frames: Vec<[f32; 2]>,
}

impl Rig {
    fn boot(pkg: Arc<Vec<u8>>, watch: Vec<(u32, u32)>) -> Rig {
        let shared = Arc::new(Shared {
            view: Mutex::new(View {
                lcd: vec![0; LCD_SIDE * LCD_SIDE], leds: [0.0; 41], pads: [0; 16], board: fm1_soc::Board::Fm1,
                booted: false, halted: false, speed: 0.0, starved: 0, late: 0, peek: Vec::new(),
            }),
            quit: Default::default(),
        });
        let params = Arc::new(fm1_plugin::Fm1Params::default());
        let (cmds, rx) = mpsc::channel();
        let (midi, midi_rx) = rtrb::RingBuffer::new(4096);
        let (audio_tx, audio) = rtrb::RingBuffer::new(16384);
        let sh = shared.clone();
        let thread = Some(std::thread::spawn(move || engine::run(sh, params, rx, midi_rx, audio_tx)));
        cmds.send(Cmd::Lead(LEAD)).unwrap();
        cmds.send(Cmd::Watch(watch)).unwrap();
        cmds.send(Cmd::Boot { pkg, sectors: Vec::new() }).unwrap();
        Rig { shared, cmds, midi, audio, thread, frames: Vec::new() }
    }

    fn now(&self) -> u64 {
        self.frames.len() as u64
    }

    fn render_until(&mut self, seconds: f64) {
        let end = (seconds * RATE) as usize;
        while self.frames.len() < end {
            match self.audio.pop() {
                Ok(f) => self.frames.push(f),
                Err(_) => std::thread::sleep(Duration::from_micros(200)),
            }
        }
    }

    fn send(&mut self, bytes: [u8; 3]) {
        let frame = self.now() + LEAD as u64;
        self.midi.push(Midi { frame, bytes, len: 3 }).unwrap();
    }

    fn press(&mut self, id: u8, at: f64) {
        self.cmds.send(Cmd::Key { id, down: true }).unwrap();
        self.render_until(at + 0.08);
        self.cmds.send(Cmd::Key { id, down: false }).unwrap();
    }

    fn panel(&self) -> Option<Panel> {
        Panel::read(&self.shared.view.lock().unwrap().peek)
    }

    fn speed(&self) -> f32 {
        self.shared.view.lock().unwrap().speed
    }

    fn knob(&mut self, k: usize, value: f32) {
        let p = self.panel().expect("the watch is set");
        for bytes in sidou::messages(sidou::target(p.lab_page, p.fx_select, k), value, 0) {
            self.send(bytes);
        }
    }

    fn screen(&self, path: &str) {
        let lcd = self.shared.view.lock().unwrap().lcd.clone();
        let rgb: Vec<u8> = lcd.iter().flat_map(|&p| {
            let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
            [(r * 255 / 31) as u8, (g * 255 / 63) as u8, (b * 255 / 31) as u8]
        }).collect();
        let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut enc = png::Encoder::new(file, LCD_SIDE as u32, LCD_SIDE as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    }

    fn stop(mut self) -> Vec<[f32; 2]> {
        self.shared.quit.store(true, Ordering::Relaxed);
        while self.audio.pop().is_ok() {}
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
        std::mem::take(&mut self.frames)
    }
}

fn note(rig: &mut Rig, key: u8, from: f64, to: f64) {
    rig.render_until(from);
    rig.send([0x90, key, 100]);
    rig.render_until(to);
    rig.send([0x80, key, 0]);
}

fn write_wav(path: &str, frames: &[[f32; 2]]) {
    let data: Vec<u8> = frames.iter().flatten().flat_map(|&s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let rate = FRAMES_PER_S as u32;
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out).unwrap();
}

fn symbol(elf: &[u8], name: &str) -> u32 {
    use object::{Object, ObjectSymbol};
    let file = object::File::parse(elf).expect("an ELF");
    file.symbols().find(|s| s.name() == Ok(name)).unwrap_or_else(|| panic!("no symbol {name}")).address() as u32
}

fn state(elf_path: &str, out: &str) {
    let elf = std::fs::read(elf_path).unwrap();
    let live = symbol(&elf, "instrument");
    let ui = symbol(&elf, "ui");
    let field = |at: u32, len: u32| serde_json::json!({ "at": at, "len": len });
    let map = serde_json::json!({
        "lab_page": field(ui + (ilp32::views::lab + ilp32::lab_view::page) as u32, 1),
        "fx_select": field(live + ilp32::live::fx_select as u32, 1),
        "preset": field(live + ilp32::live::preset as u32, 1),
        "name": field(live + ilp32::live::base_name as u32, sidou_sys::LAB_SOUND_NAME),
        "cutoff": field(live + ilp32::live::cutoff as u32, 2),
        "resonance": field(live + ilp32::live::resonance as u32, 1),
        "attack_decay": field(live + ilp32::live::attack_decay as u32, 1),
        "sustain_release": field(live + ilp32::live::sustain_release as u32, 1),
    });
    std::fs::write(out, serde_json::to_string_pretty(&map).unwrap()).unwrap();
}

fn onset_frames(frames: &[[f32; 2]], due: usize) -> Option<usize> {
    let level = |f: &[f32; 2]| f[0].abs().max(f[1].abs());
    let before = frames[due - 4410..due].iter().map(level).fold(0.0, f32::max);
    frames[due..].iter().position(|f| level(f) > before.max(0.001) * 4.0)
}

fn render(pkg_path: &str, state_path: &str, out_dir: &str) {
    let map: StateMap = serde_json::from_str(&std::fs::read_to_string(state_path).unwrap()).unwrap();
    let pkg = Arc::new(std::fs::read(pkg_path).unwrap());
    let started = Instant::now();
    let mut rig = Rig::boot(pkg, map.regions());
    rig.render_until(3.0);
    let boot_wall = started.elapsed();
    rig.screen(&format!("{out_dir}/sidou-boot.png"));
    println!("after boot: {:?}", rig.panel());
    for (i, key) in [60u8, 64, 67, 72].into_iter().enumerate() {
        let at = 3.0 + i as f64 * 0.5;
        note(&mut rig, key, at, at + 0.4);
    }
    rig.render_until(5.5);
    rig.press(ENV_BUTTON, 5.5);
    rig.render_until(6.0);
    let before = rig.panel().unwrap();
    println!("after ENV: {before:?}");
    rig.screen(&format!("{out_dir}/sidou-lab-envelope.png"));
    for (k, v) in [(0usize, 0.6f32), (1, 0.4), (2, 0.2), (3, 0.9)] {
        rig.knob(k, v);
    }
    rig.render_until(6.5);
    let after = rig.panel().unwrap();
    println!("after K1-K4: {after:?}");
    rig.screen(&format!("{out_dir}/sidou-lab-envelope-knobs.png"));
    for (i, key) in [60u8, 67].into_iter().enumerate() {
        let at = 6.5 + i as f64 * 1.0;
        note(&mut rig, key, at, at + 0.6);
    }
    rig.render_until(9.5);
    let wall = started.elapsed();
    let speed = rig.speed();
    let frames = rig.stop();
    write_wav(&format!("{out_dir}/sidou-plugin-spike.wav"), &frames);
    let audio_s = frames.len() as f64 / RATE;
    println!("boot to 3 s: {:.2} s wall", boot_wall.as_secs_f64());
    println!("rendered {audio_s:.1} s in {:.2} s: {:.1}x real time overall, engine's last-second speed {speed:.1}x", wall.as_secs_f64(), audio_s / wall.as_secs_f64());
    println!("note sent at 3.0 s, stamped {LEAD} frames later, sounds {:?} frames after its stamp", onset_frames(&frames, 3 * FRAMES_PER_S as usize + LEAD));
    println!("knobs: attack {} -> {}, decay {} -> {}, sustain {} -> {}, release {} -> {}",
        before.adsr[0], after.adsr[0], before.adsr[1], after.adsr[1], before.adsr[2], after.adsr[2], before.adsr[3], after.adsr[3]);
}

fn instances(pkg_path: &str, n: usize, seconds: f64) {
    let pkg = Arc::new(std::fs::read(pkg_path).unwrap());
    let boot_s = 3.0;
    let handles: Vec<_> = (0..n).map(|_| {
        let pkg = pkg.clone();
        std::thread::spawn(move || {
            let mut rig = Rig::boot(pkg, Vec::new());
            rig.render_until(boot_s);
            for key in [48u8, 55, 64] {
                rig.send([0x90, key, 100]);
            }
            let t = Instant::now();
            rig.render_until(boot_s + seconds);
            let rtf = seconds / t.elapsed().as_secs_f64();
            rig.stop();
            rtf
        })
    }).collect();
    let rtfs: Vec<f64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let min = rtfs.iter().cloned().fold(f64::MAX, f64::min);
    println!("{n} instances, a held chord, {seconds} s each: slowest {min:.2}x real time, all {:?}",
        rtfs.iter().map(|r| format!("{r:.2}")).collect::<Vec<_>>());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("state") => state(&args[2], &args[3]),
        Some("render") => render(&args[2], &args[3], &args[4]),
        Some("instances") => instances(&args[2], args[3].parse().unwrap(), args[4].parse().unwrap()),
        _ => eprintln!("usage: state <elf> <out.json> | render <fwsc> <state.json> <dir> | instances <fwsc> <n> <seconds>"),
    }
}
