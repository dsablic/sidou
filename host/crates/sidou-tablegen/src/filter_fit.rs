// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::f64::consts::PI;

use residfp_sys::{filter6581_dac, Model, Sampling, Sid};

pub const CLOCK_HZ: f64 = 985_248.0;
pub const FS: f64 = 44_100.0;
const SEGMENT: usize = 8192;
const SEGMENTS: usize = 7;
pub const CAPTURE: usize = SEGMENT * (SEGMENTS + 1) / 2;
pub const SETTLE_CYCLES: u32 = 250_000;
pub const SUSTAIN: u8 = 0x4;
const BAND_LO_HZ: f64 = 20.0;
const BAND_HI_HZ: f64 = 19_000.0;
const FIT_RANGE_DB: f64 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Lp,
    Bp,
    Hp,
}

pub const MODES: [Mode; 3] = [Mode::Lp, Mode::Bp, Mode::Hp];

impl Mode {
    fn bits(self) -> u8 {
        match self {
            Mode::Lp => 0x10,
            Mode::Bp => 0x20,
            Mode::Hp => 0x40,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Lp => "LP",
            Mode::Bp => "BP",
            Mode::Hp => "HP",
        }
    }

    fn model_db(self, w: f64, k: f64) -> f64 {
        let d = (1.0 - w * w).powi(2) + (k * w).powi(2);
        let num = match self {
            Mode::Lp => 0.0,
            Mode::Bp => 20.0 * libm::log10(w),
            Mode::Hp => 40.0 * libm::log10(w),
        };
        num - 10.0 * libm::log10(d)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveSetting {
    pub model: Model,
    pub curve: f64,
    pub range6581: Option<f64>,
}

impl CurveSetting {
    pub const fn default_for(model: Model) -> Self {
        Self {
            model,
            curve: 0.5,
            range6581: None,
        }
    }

    pub fn describe(&self) -> String {
        match (self.model, self.range6581) {
            (Model::Mos6581, Some(r)) => format!("6581 curve {} range {r}", self.curve),
            (Model::Mos6581, None) => format!("6581 curve {} range default", self.curve),
            (Model::Csg8580, _) => format!("8580 curve {}", self.curve),
        }
    }

    pub fn control(&self) -> Vec<f64> {
        match self.model {
            Model::Mos6581 => filter6581_dac(self.curve)
                .into_iter()
                .map(f64::from)
                .collect(),
            Model::Csg8580 => (0..2048)
                .map(|fc| if fc == 0 { 0.5 } else { f64::from(fc) })
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Response {
    pub hz: Vec<f64>,
    pub db: Vec<f64>,
}

fn new_sid(s: &CurveSetting) -> Sid {
    let mut sid = Sid::new();
    sid.set_chip_model(s.model).expect("reSIDfp chip model");
    sid.set_sampling_parameters(CLOCK_HZ, Sampling::Resample, FS)
        .expect("reSIDfp sampling parameters");
    sid.reset();
    sid.set_filter_curve(s.model, s.curve);
    sid
}

pub fn register_writes(fc: u32, res: u8, mode: Option<Mode>) -> [(u8, u8); 9] {
    let (routing, bits) = match mode {
        Some(m) => (0x01, m.bits()),
        None => (0x00, 0x00),
    };
    [
        (0x15, (fc & 7) as u8),
        (0x16, (fc >> 3) as u8),
        (0x17, (res << 4) | routing),
        (0x18, bits | 0x0f),
        (0x00, 0xff),
        (0x01, 0xff),
        (0x05, 0x00),
        (0x06, SUSTAIN << 4),
        (0x04, 0x81),
    ]
}

fn render(s: &CurveSetting, fc: u32, res: u8, mode: Option<Mode>) -> Vec<f64> {
    let mut sid = new_sid(s);
    for (reg, value) in register_writes(fc, res, mode) {
        sid.write(reg, value);
    }
    let mut buf = vec![0i16; sid.max_samples(SETTLE_CYCLES)];
    sid.clock(SETTLE_CYCLES, &mut buf);
    let mut out = Vec::with_capacity(CAPTURE + 64);
    let chunk = 20_000;
    let mut buf = vec![0i16; sid.max_samples(chunk)];
    while out.len() < CAPTURE {
        let n = sid.clock(chunk, &mut buf);
        out.extend(buf[..n].iter().map(|&v| f64::from(v)));
    }
    out.truncate(CAPTURE);
    out
}

fn fft(re: &mut [f64], im: &mut [f64], twiddle: &[(f64, f64)]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = twiddle[k * step];
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * wr - im[b] * wi;
                let ti = re[b] * wi + im[b] * wr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

pub struct Spectra(Vec<(Vec<f64>, Vec<f64>)>);

pub fn spectra(x: &[f64]) -> Spectra {
    let twiddle: Vec<(f64, f64)> = (0..SEGMENT / 2)
        .map(|k| {
            let a = -2.0 * PI * k as f64 / SEGMENT as f64;
            (libm::cos(a), libm::sin(a))
        })
        .collect();
    let window: Vec<f64> = (0..SEGMENT)
        .map(|i| 0.5 - 0.5 * libm::cos(2.0 * PI * i as f64 / SEGMENT as f64))
        .collect();
    Spectra(
        (0..SEGMENTS)
            .map(|s| {
                let seg = &x[s * SEGMENT / 2..s * SEGMENT / 2 + SEGMENT];
                let mean = seg.iter().sum::<f64>() / SEGMENT as f64;
                let mut re: Vec<f64> = seg
                    .iter()
                    .zip(&window)
                    .map(|(v, w)| (v - mean) * w)
                    .collect();
                let mut im = vec![0.0; SEGMENT];
                fft(&mut re, &mut im, &twiddle);
                re.truncate(SEGMENT / 2);
                im.truncate(SEGMENT / 2);
                (re, im)
            })
            .collect(),
    )
}

fn bands() -> Vec<(f64, usize, usize)> {
    let bin = FS / SEGMENT as f64;
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let centre = BAND_LO_HZ * libm::pow(2.0, f64::from(i) / 12.0);
        if centre > BAND_HI_HZ {
            return out;
        }
        let lo = (centre * libm::pow(2.0, -1.0 / 24.0) / bin).ceil() as usize;
        let hi = (centre * libm::pow(2.0, 1.0 / 24.0) / bin).ceil() as usize;
        if hi > lo {
            out.push((centre, lo, hi));
        }
        i += 1;
    }
}

pub fn transfer(filtered: &Spectra, direct: &Spectra) -> Response {
    let mut hz = Vec::new();
    let mut db = Vec::new();
    for (centre, lo, hi) in bands() {
        let (mut cr, mut ci, mut p) = (0.0, 0.0, 0.0);
        for ((ar, ai), (br, bi)) in filtered.0.iter().zip(&direct.0) {
            for b in lo..hi {
                cr += ar[b] * br[b] + ai[b] * bi[b];
                ci += ai[b] * br[b] - ar[b] * bi[b];
                p += br[b] * br[b] + bi[b] * bi[b];
            }
        }
        if p > 0.0 {
            hz.push(centre);
            db.push(10.0 * libm::log10((cr * cr + ci * ci) / (p * p)));
        }
    }
    Response { hz, db }
}

pub fn direct_spectra(s: &CurveSetting) -> Spectra {
    spectra(&render(s, 0, 0, None))
}

pub fn measure(s: &CurveSetting, direct: &Spectra, fc: u32, res: u8, mode: Mode) -> Response {
    transfer(&spectra(&render(s, fc, res, Some(mode))), direct)
}

#[derive(Clone, Copy, Debug)]
pub struct Fit {
    pub f0: f64,
    pub k: f64,
    pub rms_db: [f64; 3],
    pub max_db: [f64; 3],
}

fn selected(r: &Response, window: Option<f64>) -> Vec<(f64, f64)> {
    let top = r.db.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    r.hz.iter()
        .zip(&r.db)
        .filter(|(&h, &d)| {
            d >= top - FIT_RANGE_DB && window.is_none_or(|f0| h >= f0 / WINDOW && h <= f0 * WINDOW)
        })
        .map(|(&h, &d)| (h, d))
        .collect()
}

const WINDOW: f64 = 3.0;

fn cost(data: &[(Mode, Vec<(f64, f64)>)], f0: f64, k: f64) -> (f64, [f64; 3], [f64; 3]) {
    let mut total = 0.0;
    let mut rms = [0.0; 3];
    let mut max = [0.0; 3];
    for (i, (mode, points)) in data.iter().enumerate() {
        if points.is_empty() {
            continue;
        }
        let r: Vec<f64> = points
            .iter()
            .map(|&(h, d)| d - mode.model_db(h / f0, k))
            .collect();
        let level = r.iter().sum::<f64>() / r.len() as f64;
        let sq: f64 = r.iter().map(|v| (v - level).powi(2)).sum();
        total += sq;
        rms[i] = (sq / r.len() as f64).sqrt();
        max[i] = r.iter().map(|v| (v - level).abs()).fold(0.0, f64::max);
    }
    if data.iter().all(|(_, p)| p.is_empty()) {
        total = f64::INFINITY;
    }
    (total, rms, max)
}

fn minimise(data: &[(Mode, Vec<(f64, f64)>)], around: f64, k_fixed: Option<f64>) -> (f64, f64) {
    let c = |lf: f64, lk: f64| cost(data, libm::exp(lf), libm::exp(lk)).0;
    let (mut lf, mut lk) = (libm::log(around), 0.0f64);
    let mut best = f64::INFINITY;
    let ks: Vec<f64> = match k_fixed {
        Some(k) => vec![libm::log(k)],
        None => (0..64)
            .map(|i| libm::log(0.02) + f64::from(i) * libm::log(2.0) / 8.0)
            .collect(),
    };
    for i in -16..=16 {
        let f = libm::log(around) + f64::from(i) * libm::log(2.0) / 16.0;
        for &k in &ks {
            let v = c(f, k);
            if v < best {
                best = v;
                lf = f;
                lk = k;
            }
        }
    }
    let dirs: &[(f64, f64)] = if k_fixed.is_some() {
        &[(1.0, 0.0), (-1.0, 0.0)]
    } else {
        &[(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
    };
    let mut step = libm::log(2.0) / 16.0;
    while step > 1e-7 {
        let mut moved = false;
        for &(df, dk) in dirs {
            let v = c(lf + df * step, lk + dk * step);
            if v < best {
                best = v;
                lf += df * step;
                lk += dk * step;
                moved = true;
            }
        }
        if !moved {
            step /= 2.0;
        }
    }
    (libm::exp(lf), libm::exp(lk))
}

fn bp_peak(responses: &[(Mode, Response)]) -> f64 {
    let (_, r) = responses
        .iter()
        .find(|(m, _)| *m == Mode::Bp)
        .expect("a band-pass response");
    let mut best = (f64::NEG_INFINITY, r.hz[0]);
    for (&h, &d) in r.hz.iter().zip(&r.db) {
        if d > best.0 {
            best = (d, h);
        }
    }
    best.1
}

pub fn fit(responses: &[(Mode, Response)], k_fixed: Option<f64>) -> Fit {
    let pick = |f0: f64| -> Vec<(Mode, Vec<(f64, f64)>)> {
        responses
            .iter()
            .map(|(m, r)| (*m, selected(r, Some(f0))))
            .collect()
    };
    let mut f0 = bp_peak(responses);
    let mut k = 1.0;
    for _ in 0..2 {
        (f0, k) = minimise(&pick(f0), f0, k_fixed);
    }
    let (_, rms_db, max_db) = cost(&pick(f0), f0, k);
    Fit {
        f0,
        k,
        rms_db,
        max_db,
    }
}

pub fn in_reach(f0: f64) -> bool {
    f0 >= 2.0 * BAND_LO_HZ && f0 * 1.25 <= BAND_HI_HZ
}

pub fn compare(
    reference: &[(Mode, Response)],
    ours: &[(Mode, Response)],
    lo_hz: f64,
    hi_hz: f64,
) -> ([f64; 3], [f64; 3]) {
    let mut rms = [0.0; 3];
    let mut max = [0.0; 3];
    for (i, ((_, r), (_, o))) in reference.iter().zip(ours).enumerate() {
        let picked: Vec<(f64, f64)> = selected(r, None)
            .into_iter()
            .filter(|&(h, _)| h >= lo_hz && h <= hi_hz)
            .collect();
        let d: Vec<f64> = picked
            .iter()
            .map(|&(h, db)| {
                let j = o.hz.iter().position(|&x| x == h).expect("the same bands");
                o.db[j] - db
            })
            .collect();
        if d.is_empty() {
            continue;
        }
        let level = d.iter().sum::<f64>() / d.len() as f64;
        rms[i] = (d.iter().map(|v| (v - level).powi(2)).sum::<f64>() / d.len() as f64).sqrt();
        max[i] = d.iter().map(|v| (v - level).abs()).fold(0.0, f64::max);
    }
    (rms, max)
}

pub fn errors(responses: &[(Mode, Response)], f0: f64, k: f64) -> ([f64; 3], [f64; 3]) {
    let data: Vec<(Mode, Vec<(f64, f64)>)> = responses
        .iter()
        .map(|(m, r)| (*m, selected(r, Some(f0))))
        .collect();
    let (_, rms, max) = cost(&data, f0, k);
    (rms, max)
}

pub fn measure_modes(
    s: &CurveSetting,
    direct: &Spectra,
    fc: u32,
    res: u8,
) -> Vec<(Mode, Response)> {
    MODES
        .iter()
        .map(|&m| (m, measure(s, direct, fc, res, m)))
        .collect()
}

pub fn fc_grid(model: Model) -> Vec<u32> {
    let mut g: Vec<u32> = (0..2048).step_by(32).collect();
    g.push(2047);
    if model == Model::Mos6581 {
        g.push(1023);
    }
    g.sort_unstable();
    g
}

pub const GRID_RES: [u8; 3] = [0, 8, 15];
const K_FCS: usize = 6;
const K_FC_HZ: (f64, f64) = (1000.0, 10000.0);

fn parallel<T: Sync, R: Send>(jobs: Vec<T>, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..jobs.len()).map(|_| None).collect());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(job) = jobs.get(i) else { break };
                let r = f(job);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner()
        .unwrap()
        .into_iter()
        .map(|r| r.expect("every job ran"))
        .collect()
}

pub struct GridPoint {
    pub fc: u32,
    pub res: u8,
    pub responses: Vec<(Mode, Response)>,
    pub free: Fit,
    pub f0_fixed_k: f64,
}

pub struct FilterFit {
    pub setting: CurveSetting,
    pub k: [f64; 16],
    pub k_fcs: Vec<u32>,
    pub k_spread: [f64; 16],
    pub points: Vec<GridPoint>,
    pub grid_hz: Vec<(u32, Option<f64>)>,
    pub cutoff_hz: Vec<f64>,
}

fn isotonic(y: &[f64]) -> Vec<f64> {
    let mut blocks: Vec<(f64, usize)> = Vec::new();
    for &v in y {
        blocks.push((v, 1));
        while blocks.len() > 1 {
            let (b, nb) = blocks[blocks.len() - 1];
            let (a, na) = blocks[blocks.len() - 2];
            if a <= b {
                break;
            }
            blocks.pop();
            let n = na + nb;
            *blocks.last_mut().unwrap() = ((a * na as f64 + b * nb as f64) / n as f64, n);
        }
    }
    blocks
        .into_iter()
        .flat_map(|(v, n)| std::iter::repeat_n(v, n))
        .collect()
}

fn interpolate(setting: &CurveSetting, grid_hz: &[(u32, Option<f64>)]) -> Vec<f64> {
    let control = setting.control();
    let x_of = |fc: u32| {
        let c = control[fc as usize];
        if setting.model == Model::Csg8580 {
            libm::log(c)
        } else {
            c
        }
    };
    let mut pts: Vec<(f64, f64)> = grid_hz
        .iter()
        .filter_map(|&(fc, hz)| hz.map(|h| (x_of(fc), libm::log(h))))
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let ys = isotonic(&pts.iter().map(|p| p.1).collect::<Vec<_>>());
    let mut xs: Vec<(f64, f64)> = Vec::new();
    for (p, y) in pts.iter().zip(ys) {
        match xs.last_mut() {
            Some(last) if last.0 == p.0 => last.1 = last.1.max(y),
            _ => xs.push((p.0, y)),
        }
    }
    assert!(xs.len() >= 4, "too few measured cutoffs to interpolate");
    let line = |p: &[(f64, f64)]| {
        let n = p.len() as f64;
        let mx = p.iter().map(|q| q.0).sum::<f64>() / n;
        let my = p.iter().map(|q| q.1).sum::<f64>() / n;
        let sxy: f64 = p.iter().map(|q| (q.0 - mx) * (q.1 - my)).sum();
        let sxx: f64 = p.iter().map(|q| (q.0 - mx).powi(2)).sum();
        let slope = (sxy / sxx).max(0.0);
        move |x: f64| my + slope * (x - mx)
    };
    let low = line(&xs[..3]);
    let high = line(&xs[xs.len() - 3..]);
    (0..2048u32)
        .map(|fc| {
            let x = x_of(fc);
            let y = if x <= xs[0].0 {
                low(x).min(xs[0].1)
            } else if x >= xs[xs.len() - 1].0 {
                high(x).max(xs[xs.len() - 1].1)
            } else {
                let i = xs.partition_point(|p| p.0 <= x);
                let (a, b) = (xs[i - 1], xs[i]);
                a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)
            };
            libm::exp(y)
        })
        .collect()
}

fn geomean(v: impl Iterator<Item = f64>) -> Option<f64> {
    let (mut s, mut n) = (0.0, 0);
    for x in v {
        s += libm::log(x);
        n += 1;
    }
    (n > 0).then(|| libm::exp(s / f64::from(n)))
}

pub fn apply_range(setting: &CurveSetting) {
    if setting.model == Model::Mos6581 {
        Sid::new().set_filter6581_range(setting.range6581.unwrap_or(19.0 / 39.0));
    }
}

pub fn fit_setting(setting: &CurveSetting) -> FilterFit {
    apply_range(setting);
    let direct = direct_spectra(setting);
    let jobs: Vec<(u32, u8)> = fc_grid(setting.model)
        .into_iter()
        .flat_map(|fc| GRID_RES.iter().map(move |&r| (fc, r)))
        .collect();
    let measured = parallel(jobs, |&(fc, res)| {
        let responses = measure_modes(setting, &direct, fc, res);
        let free = fit(&responses, None);
        (fc, res, responses, free)
    });
    let candidates: Vec<u32> = measured
        .iter()
        .filter(|m| m.1 == 8 && m.3.f0 >= K_FC_HZ.0 && m.3.f0 <= K_FC_HZ.1)
        .map(|m| m.0)
        .collect();
    assert!(candidates.len() >= K_FCS, "too few mid-range FC values");
    let k_fcs: Vec<u32> = (0..K_FCS)
        .map(|i| candidates[i * (candidates.len() - 1) / (K_FCS - 1)])
        .collect();
    let k_jobs: Vec<(u32, u8)> = k_fcs
        .iter()
        .flat_map(|&fc| (0..16u8).map(move |r| (fc, r)))
        .collect();
    let k_fits = parallel(k_jobs, |&(fc, res)| {
        let reuse = measured.iter().find(|m| m.0 == fc && m.1 == res);
        let f = match reuse {
            Some(m) => m.3,
            None => fit(&measure_modes(setting, &direct, fc, res), None),
        };
        (res, f.k)
    });
    let mut k = [0.0; 16];
    let mut k_spread = [0.0; 16];
    for res in 0..16u8 {
        let ks: Vec<f64> = k_fits.iter().filter(|f| f.0 == res).map(|f| f.1).collect();
        k[res as usize] = geomean(ks.iter().copied()).unwrap();
        let (lo, hi) = ks
            .iter()
            .fold((f64::INFINITY, 0.0f64), |(l, h), &v| (l.min(v), h.max(v)));
        k_spread[res as usize] = hi / lo;
    }
    let points: Vec<GridPoint> = parallel(measured, |(fc, res, responses, free)| {
        let f0_fixed_k = fit(responses, Some(k[*res as usize])).f0;
        GridPoint {
            fc: *fc,
            res: *res,
            responses: responses.clone(),
            free: *free,
            f0_fixed_k,
        }
    });
    let grid_hz: Vec<(u32, Option<f64>)> = fc_grid(setting.model)
        .into_iter()
        .map(|fc| {
            let hz = geomean(
                points
                    .iter()
                    .filter(|p| p.fc == fc && in_reach(p.f0_fixed_k))
                    .map(|p| p.f0_fixed_k),
            );
            let all_in = points
                .iter()
                .filter(|p| p.fc == fc)
                .all(|p| in_reach(p.f0_fixed_k));
            (fc, hz.filter(|_| all_in))
        })
        .collect();
    let cutoff_hz = interpolate(setting, &grid_hz);
    FilterFit {
        setting: *setting,
        k,
        k_fcs,
        k_spread,
        points,
        grid_hz,
        cutoff_hz,
    }
}

pub struct ErrorSummary {
    pub res: u8,
    pub mode: Mode,
    pub rms_db: f64,
    pub max_db: f64,
    pub points: usize,
}

impl FilterFit {
    pub fn errors(&self) -> Vec<ErrorSummary> {
        let mut out = Vec::new();
        for res in GRID_RES {
            let per: Vec<([f64; 3], [f64; 3])> = self
                .points
                .iter()
                .filter(|p| {
                    p.res == res && self.grid_hz.iter().any(|g| g.0 == p.fc && g.1.is_some())
                })
                .map(|p| {
                    errors(
                        &p.responses,
                        self.cutoff_hz[p.fc as usize],
                        self.k[res as usize],
                    )
                })
                .collect();
            for (i, &mode) in MODES.iter().enumerate() {
                let n = per.len();
                let rms = (per.iter().map(|e| e.0[i].powi(2)).sum::<f64>() / n as f64).sqrt();
                let max = per.iter().map(|e| e.1[i]).fold(0.0, f64::max);
                out.push(ErrorSummary {
                    res,
                    mode,
                    rms_db: rms,
                    max_db: max,
                    points: n,
                });
            }
        }
        out
    }

    pub fn report(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        writeln!(s, "{}", self.setting.describe()).unwrap();
        writeln!(
            s,
            "  damping k = 1/Q (FC {:?}); spread = largest / smallest:",
            self.k_fcs
        )
        .unwrap();
        for (r, (k, sp)) in self.k.iter().zip(&self.k_spread).enumerate() {
            writeln!(
                s,
                "    res {r:2}: k {k:.4} (Q {:.3}), spread {sp:.3}",
                1.0 / k
            )
            .unwrap();
        }
        writeln!(
            s,
            "  cutoff per grid FC (fixed-k fits at res 0/8/15; free fits' k):"
        )
        .unwrap();
        for &(fc, hz) in &self.grid_hz {
            let per: Vec<String> = self
                .points
                .iter()
                .filter(|p| p.fc == fc)
                .map(|p| format!("{:.0}/{:.3}", p.f0_fixed_k, p.free.k))
                .collect();
            let shown = hz.map_or("out of reach".to_owned(), |h| format!("{h:.1} Hz"));
            writeln!(
                s,
                "    fc {fc:4}: {shown:>14} -> table {:8.1}  [{}]",
                self.cutoff_hz[fc as usize],
                per.join(", ")
            )
            .unwrap();
        }
        writeln!(
            s,
            "  error of the tables against reSIDfp around the cutoff (dB):"
        )
        .unwrap();
        for e in self.errors() {
            writeln!(
                s,
                "    res {:2} {}: rms {:.2}, max {:.2} ({} FC values)",
                e.res,
                e.mode.name(),
                e.rms_db,
                e.max_db,
                e.points
            )
            .unwrap();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isotonic_pools_violators() {
        assert_eq!(isotonic(&[1.0, 3.0, 2.0, 4.0]), vec![1.0, 2.5, 2.5, 4.0]);
        assert_eq!(isotonic(&[3.0, 2.0, 1.0]), vec![2.0, 2.0, 2.0]);
        assert_eq!(isotonic(&[1.0, 2.0]), vec![1.0, 2.0]);
    }

    #[test]
    fn fft_matches_a_direct_dft() {
        let n = 64;
        let x: Vec<f64> = (0..n).map(|i| ((i * 7919) % 13) as f64 - 6.0).collect();
        let twiddle: Vec<(f64, f64)> = (0..n / 2)
            .map(|k| {
                let a = -2.0 * PI * k as f64 / n as f64;
                (libm::cos(a), libm::sin(a))
            })
            .collect();
        let (mut re, mut im) = (x.clone(), vec![0.0; n]);
        fft(&mut re, &mut im, &twiddle);
        for k in 0..n {
            let (mut r, mut i) = (0.0, 0.0);
            for (j, &v) in x.iter().enumerate() {
                let a = -2.0 * PI * (k * j) as f64 / n as f64;
                r += v * libm::cos(a);
                i += v * libm::sin(a);
            }
            assert!(
                (re[k] - r).abs() < 1e-9 && (im[k] - i).abs() < 1e-9,
                "bin {k}"
            );
        }
    }

    #[test]
    fn fit_recovers_a_state_variable_filter() {
        let (f0, k) = (1234.5, 0.6);
        let hz: Vec<f64> = (0..120)
            .map(|i| 20.0 * libm::pow(2.0, f64::from(i) / 12.0))
            .collect();
        let responses: Vec<(Mode, Response)> = MODES
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let db = hz
                    .iter()
                    .map(|&h| m.model_db(h / f0, k) - 3.0 * i as f64)
                    .collect();
                (m, Response { hz: hz.clone(), db })
            })
            .collect();
        let f = fit(&responses, None);
        assert!((f.f0 / f0 - 1.0).abs() < 1e-5, "f0 {}", f.f0);
        assert!((f.k / k - 1.0).abs() < 1e-5, "k {}", f.k);
        assert!(f.max_db.iter().all(|&e| e < 1e-3));
        let fixed = fit(&responses, Some(k));
        assert!((fixed.f0 / f0 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_8580_cutoff_interpolates_in_proportion_to_fc() {
        let s = CurveSetting::default_for(Model::Csg8580);
        let grid: Vec<(u32, Option<f64>)> = fc_grid(Model::Csg8580)
            .into_iter()
            .map(|fc| (fc, (32..=1600).contains(&fc).then(|| 7.0 * f64::from(fc))))
            .collect();
        let hz = interpolate(&s, &grid);
        for fc in [1u32, 33, 500, 1000, 2047] {
            assert!(
                (hz[fc as usize] / (7.0 * f64::from(fc)) - 1.0).abs() < 1e-9,
                "fc {fc}"
            );
        }
        assert!((hz[0] / 3.5 - 1.0).abs() < 1e-9);
    }
}
