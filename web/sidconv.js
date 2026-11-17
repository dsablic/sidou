// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
export const FRAME_CYCLES = { pal: 19656, ntsc: 17095 };
export const PAL_CLOCK = 985248, NTSC_CLOCK = 1022727;
export const VOICES_A_CHIP = 3;
export const MAX_INSTRUMENTS_A_VOICE = 6;
export const GT2_MAX_INSTRUMENTS = 63;

const REG = { FREQ: 0, PW: 2, CTRL: 4, AD: 5, SR: 6, VOICE: 7, CUT_LO: 21, CUT_HI: 22, RES: 23, MODE: 24 };
const CTRL = { GATE: 1, TEST: 8, PULSE: 0x40, NOISE: 0x80, WAVES: 0xf0 };
const NOTE = { FIRST: 0x60, REST: 0xbd, OFF: 0xbe, END: 0xff, HIGHEST: 92 };
const CMD = { TIE: 3, FUNK: 0xe, TEMPO: 0xf };
const TIE_SPEED = 0;
const WAVE = { SAME: 0x00, KEEP: 0x80, JUMP: 0xff, STOP: 0x00, ABS: 0x80, DOWN: 0x80 };
const PULSE_SET = 0x80, FILTER_SET = 0x80, FILTER_CUT = 0x00;
const FIRST_WAVE_DEFAULT = 0x09, GATE_TIMER = 2, HR_ADSR = "$0F00";
const MIN_TEMPO = 3, MAX_TEMPO = 0x7f, MAX_ROW_FRAMES = 16;
const GRID_FIT = 0.9, GRID_WINDOW = 12, GRID_SLACK = 0.01, MISS_COST = 200, CHANGE_COST = 16, SEGMENT_HITS = 8, CUT_GAIN = 0.8;
const BASE_FRAMES = 16, GLIDE_STEP = 2, VIBRATO_TURNS = 5, VIBRATO_RANGE = 0.06, VIBRATO_EVEN = 1.5, PULSE_RUNS = 2, TRANSIENT_FRAMES = 8, ARP_PERIOD_MAX = 8, ARP_FIT = 0.9, LEGATO_MIN_FRAMES = 3;
const PATTERN_LENGTHS = [16, 32, 64], ORDER_ENTRY_COST = 4;
const TRANSPOSE_DOWN = 15, TRANSPOSE_UP = 14, TRANSPOSE_ZERO = 0xf0, REPEAT = 0xd0, REPEAT_MAX = 16;
const DIGI_VOLUME_WRITES = 4;
const TABLE_MAX = 255, PATTERN_MAX = 208, ORDER_MAX = 254;
const SEMITONES = 12, LAG_MAX = 25;
const NAME_BYTES = 32, INSTRUMENT_NAME = 16;

export class ConvertError extends Error {
  constructor(message) {
    super(message);
    this.name = "ConvertError";
  }
}

export function framesOfCapture(run) {
  const cycles = run.ntsc ? FRAME_CYCLES.ntsc : FRAME_CYCLES.pal;
  const count = Math.round((run.seconds * run.clock) / cycles);
  const chips = Array.from({ length: run.chips }, () => Array.from({ length: count }, () => []));
  for (const c of run.calls) {
    for (const [cycle, chip, reg, value] of c.writes) {
      const f = Math.floor((c.start + cycle) / cycles);
      if (f < count && chip < run.chips) chips[chip][f].push([reg, value]);
    }
  }
  return chips;
}

export function framesOfPlayer(played) {
  return [played.frames.map((f) => f.writes)];
}

export function tracksOf(chipFrames) {
  const regs = new Array(32).fill(0);
  const voices = Array.from({ length: VOICES_A_CHIP }, () => []);
  const chip = [];
  for (const writes of chipFrames) {
    const rose = [false, false, false], fell = [false, false, false];
    let volumeWrites = 0;
    for (const [reg, value] of writes) {
      if (reg < VOICES_A_CHIP * REG.VOICE && reg % REG.VOICE === REG.CTRL) {
        const v = Math.floor(reg / REG.VOICE), was = regs[reg] & CTRL.GATE, now = value & CTRL.GATE;
        if (!was && now) rose[v] = true;
        if (was && !now) fell[v] = true;
      }
      if (reg === REG.MODE) volumeWrites++;
      regs[reg] = value;
    }
    voices.forEach((frames, v) => {
      const o = v * REG.VOICE;
      frames.push({
        freq: regs[o + REG.FREQ] | (regs[o + REG.FREQ + 1] << 8),
        pw: regs[o + REG.PW] | ((regs[o + REG.PW + 1] & 0x0f) << 8),
        ctrl: regs[o + REG.CTRL], ad: regs[o + REG.AD], sr: regs[o + REG.SR],
        rose: rose[v], fell: fell[v] && !(regs[o + REG.CTRL] & CTRL.GATE),
      });
    });
    chip.push({ cut: regs[REG.CUT_HI], res: regs[REG.RES], mode: regs[REG.MODE], volumeWrites });
  }
  return { voices, chip };
}

export function makePitch(freqTable, clock) {
  const logs = freqTable.map((f) => Math.log2(f * PAL_CLOCK));
  return (freq) => {
    if (freq === 0) return null;
    const l = Math.log2(freq * clock);
    const n = Math.round((l - logs[0]) * SEMITONES);
    return Math.max(0, Math.min(freqTable.length - 1, n));
  };
}

const audible = (f) => (f.ctrl & CTRL.WAVES) !== 0 && !(f.ctrl & CTRL.TEST);
const tonal = (f) => audible(f) && !(f.ctrl & CTRL.NOISE);

const baseOf = (pitches) => {
  const heard = pitches.filter((p) => p !== null);
  for (let i = 0; i + 1 < heard.length; i++) {
    if (Math.abs(heard[i + 1] - heard[i]) <= GLIDE_STEP) return heard[i];
  }
  return heard[0] ?? null;
};

const runsOf = (values) => {
  const runs = [];
  for (let i = 1; i < values.length; i++) {
    const d = values[i] - values[i - 1];
    if (d === 0) continue;
    const last = runs[runs.length - 1];
    if (last && Math.sign(last.sum) === Math.sign(d)) {
      last.sum += d;
      last.changes++;
    } else {
      runs.push({ sum: d, changes: 1, at: i });
    }
  }
  return runs;
};

const median = (xs) => [...xs].sort((a, b) => a - b)[xs.length >> 1];

const vibratoOf = (freqs) => {
  const runs = runsOf(freqs);
  if (runs.length < VIBRATO_TURNS) return null;
  const swings = runs.slice(1, -1).map((r) => Math.abs(r.sum));
  if (Math.max(...swings) > Math.min(...swings) * VIBRATO_EVEN) return null;
  const center = median(freqs);
  if (freqs.some((f) => Math.abs(f - center) > center * VIBRATO_RANGE)) return null;
  return { center };
};

export function notesOf(frames, pitch) {
  const onsets = [];
  frames.forEach((f, i) => { if (f.rose) onsets.push(i); });
  return onsets.map((start, k) => {
    const next = k + 1 < onsets.length ? onsets[k + 1] : frames.length;
    let end = next;
    for (let i = start + 1; i < next; i++) if (frames[i].fell) { end = i; break; }
    const body = frames.slice(start, next);
    const pitches = body.map((f) => (tonal(f) ? pitch(f.freq) : null));
    const heard = body.map((f) => (audible(f) ? pitch(f.freq) : null));
    const vibrato = vibratoOf(body.slice(TRANSIENT_FRAMES + 1, end - start).filter(tonal).map((f) => f.freq));
    const note = vibrato ? pitch(vibrato.center) : baseOf(pitches.slice(1, Math.max(2, Math.min(end - start, BASE_FRAMES)))) ?? heard.find((p) => p !== null);
    return { start, end, next, body, pitches, heard, vibrato, note: note ?? null };
  }).filter((n) => n.note !== null);
}

const CANDIDATES = (() => {
  const out = [];
  for (let t = MAX_ROW_FRAMES; t >= MIN_TEMPO; t--) out.push([t]);
  for (let sum = MAX_ROW_FRAMES * 2; sum >= MIN_TEMPO * 2; sum--) {
    for (let a = Math.floor(sum / 2) + 1; sum - a >= MIN_TEMPO && a <= MAX_ROW_FRAMES; a++) out.push([a, sum - a], [sum - a, a]);
  }
  return out;
})();

const walk = (onsets, from, lengths) => {
  let row = onsets[from], r = 0, hits = 0, misses = 0, pending = 0, last = from;
  const recent = [];
  for (let k = from; k < onsets.length; k++) {
    while (row < onsets[k]) row += lengths[r++ % lengths.length];
    const hit = row === onsets[k];
    recent.push(hit);
    if (recent.length > GRID_WINDOW) recent.shift();
    if (recent.filter((x) => !x).length > GRID_WINDOW * (1 - GRID_FIT)) break;
    if (hit) { hits++; misses += pending; pending = 0; last = k; } else pending++;
  }
  return { hits, misses, last, rows: (onsets[last] - onsets[from]) / meanLength(lengths) + 1 };
};

const fitOf = (onsets, lengths) => {
  const pair = lengths.reduce((x, y) => x + y, 0);
  const marks = new Set(lengths.length > 1 ? [0, lengths[0]] : [0]);
  let best = { fit: -1, phase: 0 };
  for (let phase = 0; phase < pair; phase++) {
    const fit = onsets.filter((f) => marks.has((((f - phase) % pair) + pair) % pair)).length / onsets.length;
    if (fit > best.fit) best = { fit, phase };
  }
  return best;
};

const meanLength = (lengths) => lengths.reduce((x, y) => x + y, 0) / lengths.length;

const coarser = (x, y) => {
  const d = meanLength(y.lengths) - meanLength(x.lengths);
  return d > 0 || (d === 0 && y.lengths.length < x.lengths.length) ? y : x;
};

const costOf = (rows, misses) => rows + MISS_COST * misses;

const wholePlan = (all) => {
  const whole = CANDIDATES.map((lengths) => ({ lengths, ...fitOf(all, lengths) }));
  const top = Math.max(...whole.map((t) => t.fit));
  const pick = whole.filter((t) => t.fit >= top - GRID_SLACK).reduce(coarser);
  const start = all.find((f) => fitOf([f], pick.lengths).phase === pick.phase) ?? all[0];
  const misses = Math.round((1 - pick.fit) * all.length);
  return { segments: [{ start, lengths: pick.lengths }], cost: costOf((all[all.length - 1] - start) / meanLength(pick.lengths), misses) };
};

const segmentedPlan = (all) => {
  const segments = [];
  let cost = 0;
  for (let from = 0; from < all.length;) {
    const tries = CANDIDATES.filter((lengths) => lengths.length === 1).map((lengths) => ({ lengths, ...walk(all, from, lengths) }));
    const enough = Math.min(SEGMENT_HITS, all.length - from);
    const value = (t) => t.hits / costOf(t.rows, t.misses);
    const fit = tries.filter((t) => t.hits >= enough);
    if (fit.length === 0) return { segments, cost: Infinity };
    const pick = fit.reduce((x, y) => (value(y) > value(x) || (value(y) === value(x) && coarser(x, y) === y) ? y : x));
    cost += costOf(pick.rows, pick.misses) + (segments.length ? CHANGE_COST : 0);
    segments.push({ start: all[from], lengths: pick.lengths });
    from = pick.last + 1;
  }
  return { segments, cost };
};

export function segmentsOf(onsets) {
  const all = [...new Set(onsets)].sort((a, b) => a - b);
  if (all.length === 0) return [{ start: 0, lengths: [6] }];
  const whole = wholePlan(all), cut = segmentedPlan(all);
  return (cut.cost < whole.cost * CUT_GAIN ? cut : whole).segments;
}

export function rowStarts(segments, frames) {
  const first = segments[0];
  const pair = first.lengths.reduce((x, y) => x + y, 0);
  const rows = [];
  segments.forEach((seg, k) => {
    const until = k + 1 < segments.length ? segments[k + 1].start : frames;
    let at = k === 0 ? first.start % pair : seg.start;
    for (let r = 0; at < until; r++) {
      rows.push({ at, lengths: seg.lengths, first: r === 0 });
      at += seg.lengths[r % seg.lengths.length];
    }
  });
  return rows.filter((row, i) => i + 1 >= rows.length || rows[i + 1].at - row.at >= MIN_TEMPO);
}

export function rowIndex(starts) {
  return (frame) => {
    let lo = 0, hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= frame) lo = mid; else hi = mid - 1;
    }
    const next = starts[lo + 1];
    return next !== undefined && next - frame < frame - starts[lo] ? lo + 1 : lo;
  };
}

const periodOf = (offsets) => {
  for (let p = 1; p <= ARP_PERIOD_MAX; p++) {
    if (offsets.length < p * 2) break;
    let same = 0;
    for (let i = 0; i + p < offsets.length; i++) if (offsets[i] === offsets[i + p]) same++;
    if (same / (offsets.length - p) >= ARP_FIT) return p;
  }
  return 0;
};

const encodeRelative = (o) => (o >= 0 ? Math.min(o, 0x5f) : Math.max(0x60, WAVE.DOWN + o));
const encodeAbsolute = (n) => WAVE.ABS + Math.max(1, Math.min(n, 0x5f));

export function shapeOf(n) {
  const len = n.end - n.start;
  const steady = n.pitches.slice(1).map((p) => (p === null ? null : p - n.note));
  const transient = Math.min(TRANSIENT_FRAMES, n.body.length - 1);
  const head = [];
  for (let i = 1; i <= transient; i++) {
    const f = n.body[i];
    head.push({ ctrl: f.ctrl, pitch: f.ctrl & CTRL.NOISE && n.heard[i] !== null ? encodeAbsolute(n.heard[i]) : null, rel: steady[i - 1] ?? null });
  }
  const after = steady.slice(transient).filter((o) => o !== null);
  const period = periodOf(after);
  const loop = period > 1 && new Set(after.slice(0, period)).size > 1 ? after.slice(0, period) : null;
  const sounding = n.body.slice(1, Math.max(2, len));
  const pulsed = sounding.find((f) => f.ctrl & CTRL.PULSE) ?? sounding[0] ?? n.body[0];
  const sweep = runsOf(sounding.slice(sounding.indexOf(pulsed)).map((f) => f.pw)).slice(0, PULSE_RUNS).map((r) => ({ time: Math.min(0x7f, r.changes), speed: Math.max(-128, Math.min(127, Math.round(r.sum / r.changes))) }));
  return { first: n.body[0].ctrl, ad: n.body[0].ad, sr: n.body[0].sr, head, loop, pw: pulsed.pw, sweep, len };
}

const keyOf = (s) => {
  const head = s.head.map((h) => `${h.ctrl.toString(16)}${h.pitch !== null ? `@${h.pitch}` : `+${h.rel ?? "x"}`}`).join(" ");
  return `${s.first}|${s.ad}|${s.sr}|${head}|${s.loop ? s.loop.join(",") : "-"}|${s.pw >> 8}`;
};

const distance = (a, b) => {
  let d = 0;
  for (const [x, y] of [[a.ad, b.ad], [a.sr, b.sr]]) d += Math.abs((x >> 4) - (y >> 4)) + Math.abs((x & 15) - (y & 15));
  const n = Math.max(a.head.length, b.head.length);
  for (let i = 0; i < n; i++) {
    const p = a.head[i], q = b.head[i];
    if (!p || !q) d += 2; else if ((p.ctrl & CTRL.WAVES) !== (q.ctrl & CTRL.WAVES)) d += 4; else if (p.rel !== q.rel) d += 1;
  }
  return d + (String(a.loop) === String(b.loop) ? 0 : 6) + Math.abs((a.pw >> 8) - (b.pw >> 8)) / 4;
};

export function clusterShapes(shapes, cap = MAX_INSTRUMENTS_A_VOICE) {
  const counts = new Map();
  shapes.forEach((s) => {
    const k = keyOf(s);
    const c = counts.get(k) ?? { key: k, shape: s, count: 0 };
    c.count++;
    if (s.len > c.shape.len) c.shape = s;
    counts.set(k, c);
  });
  const kept = [...counts.values()].sort((a, b) => b.count - a.count).slice(0, cap);
  const pick = (s) => {
    const own = kept.findIndex((c) => c.key === keyOf(s));
    if (own >= 0) return own;
    let best = 0;
    kept.forEach((c, i) => { if (distance(c.shape, s) < distance(kept[best].shape, s)) best = i; });
    return best;
  };
  return { kept: kept.map((c) => c.shape), keys: kept.map((c) => c.key), assign: shapes.map(pick) };
}

class Tables {
  constructor() {
    this.left = [[], [], [], []];
    this.right = [[], [], [], []];
  }

  add(t, rows) {
    const start = this.left[t].length + 1;
    if (start - 1 + rows.length > TABLE_MAX) return 0;
    rows.forEach(([l, r]) => {
      this.left[t].push(l);
      this.right[t].push(typeof r === "function" ? r(start) : r);
    });
    return start;
  }

  speed(left, right) {
    for (let i = 0; i < this.left[3].length; i++) if (this.left[3][i] === left && this.right[3][i] === right) return i + 1;
    return this.add(3, [[left, right]]);
  }
}

const waveRows = (s) => {
  const rows = [];
  let wave = -1;
  for (const h of s.head) {
    const w = h.ctrl === wave ? WAVE.SAME : h.ctrl;
    wave = h.ctrl;
    rows.push([w === WAVE.SAME && rows.length === 0 ? h.ctrl : w, h.pitch ?? (h.rel === null ? WAVE.KEEP : encodeRelative(h.rel))]);
  }
  if (rows.length === 0) rows.push([s.first & ~CTRL.TEST, 0]);
  if (s.loop) {
    const at = rows.length;
    s.loop.forEach((o) => rows.push([WAVE.SAME, encodeRelative(o)]));
    rows.push([WAVE.JUMP, (start) => start + at]);
  } else {
    rows.push([WAVE.JUMP, WAVE.STOP]);
  }
  return rows;
};

const pulseRows = (s) => {
  const rows = [[PULSE_SET | ((s.pw >> 8) & 0x0f), s.pw & 0xff]];
  s.sweep.forEach((r) => rows.push([r.time, r.speed & 0xff]));
  rows.push([WAVE.JUMP, s.sweep.length ? (start) => start + 1 : 0]);
  return rows;
};

const filterRows = (f) => [[FILTER_SET | (f.mode & 0x70), f.res], [FILTER_CUT, f.cut], [WAVE.JUMP, 0]];

const firstWave = (ctrl) => (ctrl & CTRL.TEST && ctrl & CTRL.GATE ? ctrl : FIRST_WAVE_DEFAULT);

export function analyse(chipFrames, { pitch, voices: wanted }) {
  const tracks = chipFrames.map(tracksOf);
  const voices = [];
  tracks.forEach((t, chip) => t.voices.forEach((frames, v) => voices.push({ chip, v, frames, notes: notesOf(frames, pitch) })));
  const used = wanted ?? (voices.length > VOICES_A_CHIP && voices.slice(VOICES_A_CHIP).some((x) => x.notes.length) ? voices.length : VOICES_A_CHIP);
  return { tracks, voices: voices.slice(0, used) };
}

const filterVoice = (a) => {
  let best = { score: 0, index: -1 };
  a.voices.forEach((x, i) => {
    const chip = a.tracks[x.chip].chip;
    const score = x.notes.filter((n) => chip[n.start] && chip[n.start].res & (1 << x.v)).length;
    if (score > best.score) best = { score, index: i };
  });
  return best.index;
};

const legatoOf = (n, rowAt) => {
  const out = [];
  let run = 0, cur = n.note;
  for (let i = TRANSIENT_FRAMES; i < n.end - n.start; i++) {
    const p = n.pitches[i];
    if (p === null || p === cur) { run = 0; continue; }
    if (++run >= LEGATO_MIN_FRAMES) {
      const at = n.start + i - run + 1;
      out.push({ row: rowAt(at), note: p });
      cur = p;
      run = 0;
    }
  }
  return out;
};

export function buildSong(a, { timeline, name = "", author = "", released = "" }) {
  const starts = timeline.map((r) => r.at);
  const rows = starts.length;
  const rowAt = rowIndex(starts);
  const tempo = Math.round(meanLength(timeline[0].lengths));
  const tables = new Tables();
  const instruments = [];
  const keyIndex = new Map();
  const filterAt = filterVoice(a);
  const dropped = new Set();
  const channels = a.voices.map((x, vi) => {
    const shapes = x.notes.map(shapeOf);
    const { kept, keys, assign } = clusterShapes(shapes);
    const numbers = kept.map((s, k) => {
      const key = `${keys[k]}|${vi === filterAt ? "f" : ""}`;
      if (keyIndex.has(key)) return keyIndex.get(key);
      if (instruments.length >= GT2_MAX_INSTRUMENTS) { dropped.add("instruments past GoatTracker's 63"); return instruments.length; }
      const longest = x.notes[assign.indexOf(k)];
      const chipState = a.tracks[x.chip].chip[longest.start];
      const wave = tables.add(0, waveRows(s));
      const pulse = s.head.some((h) => h.ctrl & CTRL.PULSE) || s.first & CTRL.PULSE ? tables.add(1, pulseRows(s)) : 0;
      const filter = vi === filterAt && chipState ? tables.add(2, filterRows(chipState)) : 0;
      if (!wave) dropped.add("wave table rows past 255");
      instruments.push({ ad: s.ad, sr: s.sr, wave, pulse, filter, vibrato: 0, vibDelay: 0, gate: GATE_TIMER,
        first: firstWave(s.first), name: `${String.fromCharCode(65 + x.chip)}${x.v + 1} ${instruments.length + 1}` });
      keyIndex.set(key, instruments.length);
      return instruments.length;
    });
    const cells = Array.from({ length: rows }, () => [NOTE.REST, 0, 0, 0]);
    let last = 0;
    x.notes.forEach((n, k) => {
      const r = rowAt(n.start);
      if (r < 0 || r >= rows) return;
      const ins = numbers[assign[k]];
      cells[r] = [NOTE.FIRST + Math.min(NOTE.HIGHEST, n.note), ins === last ? 0 : ins, 0, 0];
      last = ins;
      for (const l of legatoOf(n, rowAt)) if (l.row > r && l.row < rows && cells[l.row][0] === NOTE.REST) cells[l.row] = [NOTE.FIRST + Math.min(NOTE.HIGHEST, l.note), 0, CMD.TIE, TIE_SPEED];
      if (n.end < n.next && n.end - n.start > TRANSIENT_FRAMES) {
        const off = rowAt(n.end + GATE_TIMER);
        if (off > r && off < rows && cells[off][0] === NOTE.REST) cells[off] = [NOTE.OFF, 0, 0, 0];
      }
    });
    return cells;
  });
  const changes = placeTempo(channels, tempoCells(timeline, tables));
  const song = layout(channels);
  return { name, author, released, channels: channels.length, tempo, swing: timeline[0].lengths.length > 1 ? timeline[0].lengths : null,
    changes, instruments, tables, ...song, dropped: [...dropped] };
}

const tempoCells = (timeline, tables) => {
  let now = null;
  return timeline.map((row, i) => {
    if (row.lengths.length > 1) {
      if (!row.first && now === String(row.lengths)) return null;
      now = String(row.lengths);
      return [CMD.FUNK, tables.speed(row.lengths[0], row.lengths[1])];
    }
    const next = i + 1 < timeline.length ? timeline[i + 1].at : row.at + row.lengths[0];
    const length = Math.max(MIN_TEMPO, Math.min(MAX_TEMPO, next - row.at));
    if (now === length) return null;
    now = length;
    return [CMD.TEMPO, length];
  });
};

const placeTempo = (channels, cells) => {
  let changes = 0;
  cells.forEach((cell, r) => {
    if (!cell) return;
    const free = channels.find((c) => c[r][2] === 0) ?? channels[0];
    free[r][2] = cell[0];
    free[r][3] = cell[1];
    changes++;
  });
  return changes;
};

const cellsKey = (cells) => cells.map((c) => c.join(".")).join(" ");

const shifted = (cells) => {
  const first = cells.find((c) => c[0] >= NOTE.FIRST && c[0] < NOTE.REST);
  if (!first) return { key: cellsKey(cells), base: 0 };
  const base = first[0];
  return { key: cells.map((c) => (c[0] >= NOTE.FIRST && c[0] < NOTE.REST ? [c[0] - base, ...c.slice(1)] : c).join(".")).join(" "), base };
};

const layoutWith = (channels, length) => {
  const patterns = [];
  const index = new Map();
  const orders = channels.map((cells) => {
    const entries = [];
    let transpose = 0, prev = null, repeat = 0;
    const flush = () => {
      if (prev === null) return;
      if (prev.transpose !== transpose) { entries.push(TRANSPOSE_ZERO + prev.transpose); transpose = prev.transpose; }
      if (repeat > 1) entries.push(REPEAT + ((repeat - 1) & 0x0f));
      entries.push(prev.pattern);
    };
    for (let at = 0; at < cells.length; at += length) {
      const chunk = cells.slice(at, at + length);
      const { key, base } = shifted(chunk);
      let hit = index.get(key), t = 0;
      if (hit) {
        t = base - hit.base;
        if (t < -TRANSPOSE_DOWN || t > TRANSPOSE_UP) hit = null;
      }
      if (!hit) {
        hit = { pattern: patterns.length, base };
        if (!index.has(key)) index.set(key, hit);
        patterns.push(chunk);
        t = 0;
      }
      const entry = { pattern: hit.pattern, transpose: t };
      if (prev && prev.pattern === entry.pattern && prev.transpose === entry.transpose && repeat < REPEAT_MAX) {
        repeat++;
      } else {
        flush();
        prev = entry;
        repeat = 1;
      }
    }
    flush();
    return entries;
  });
  const rowsKept = patterns.reduce((s, p) => s + p.length, 0);
  return { patterns, orders, cost: rowsKept + ORDER_ENTRY_COST * orders.reduce((s, o) => s + o.length, 0) };
};

const layout = (channels) => {
  const fits = PATTERN_LENGTHS.map((l) => layoutWith(channels, l))
    .filter((l) => l.patterns.length <= PATTERN_MAX && l.orders.every((o) => o.length <= ORDER_MAX));
  if (fits.length === 0) throw new ConvertError("the tune is too long for a GoatTracker song: choose a shorter length");
  const best = fits.reduce((a, b) => (b.cost < a.cost ? b : a));
  return { patterns: best.patterns, orders: best.orders };
};

const padded = (s, n) => {
  const b = new Uint8Array(n);
  const bytes = new TextEncoder().encode(String(s).replace(/[^\x20-\x7e]/g, "?"));
  b.set(bytes.subarray(0, n));
  return b;
};

export function writeSng(song) {
  const out = [];
  const push = (...xs) => xs.forEach((x) => (typeof x === "number" ? out.push(x & 0xff) : out.push(...x)));
  push(..."GTS5".split("").map((c) => c.charCodeAt(0)));
  push(padded(song.name, NAME_BYTES), padded(song.author, NAME_BYTES), padded(song.released, NAME_BYTES), 1);
  for (const o of song.orders) push(o.length + 1, o, 0xff, 0);
  push(song.instruments.length);
  for (const i of song.instruments) push(i.ad, i.sr, i.wave, i.pulse, i.filter, i.vibrato, i.vibDelay, i.gate, i.first, padded(i.name, INSTRUMENT_NAME));
  for (let t = 0; t < 4; t++) push(song.tables.left[t].length, song.tables.left[t], song.tables.right[t]);
  push(song.patterns.length);
  for (const p of song.patterns) {
    push(p.length + 1);
    p.forEach((c) => push(c));
    push(NOTE.END, 0, 0, 0);
  }
  return Uint8Array.from(out);
}

export function previewSong(song) {
  return writeSng({ ...song, channels: VOICES_A_CHIP, orders: song.orders.slice(0, VOICES_A_CHIP) });
}

export function simplifications(a, song, run) {
  const out = [];
  const anyVolumeDigi = a.tracks.some((t) => t.chip.some((c) => c.volumeWrites >= DIGI_VOLUME_WRITES));
  if (anyVolumeDigi) out.push("digi samples (fast volume-register writes) are dropped");
  out.push(`hard restarts are GoatTracker's own: gate off ${GATE_TIMER} frames before each note, ADSR ${HR_ADSR}`);
  out.push(`${song.swing ? `swing as funktempo ${song.swing.join("/")}` : `tempo ${song.tempo} (frames a row)`}${song.changes > 1 ? `, with ${song.changes - 1} tempo changes` : ""}; notes off the row grid move to the nearest row`);
  out.push(`instruments: at most ${MAX_INSTRUMENTS_A_VOICE} a voice, each note given the closest; ring and sync detail, filter sweeps and pulse sweeps past their first turn are simplified`);
  const vibrato = a.voices.reduce((k, x) => k + x.notes.filter((n) => n.vibrato).length, 0);
  if (vibrato) out.push(`vibrato is dropped: ${vibrato} notes play at their centre pitch`);
  if (a.tracks.length > 1 && song.channels === VOICES_A_CHIP) out.push("the second chip is silent in this sub-tune and is left out");
  if (song.channels > VOICES_A_CHIP) out.push("six voices: a GoatTracker Stereo song; the FM-1 plays voices 1-3 (chip A) and keeps 4-6");
  if (run.dropped) out.push(`${run.dropped} SID writes in a single frame past the capture's room were dropped`);
  if (run.stopped) out.push(`the tune stopped early: ${run.stopped}`);
  song.dropped.forEach((d) => out.push(`dropped: ${d}`));
  return out;
}

export function convert(run, { freqTable, name, author, released }) {
  const clock = run.clock;
  const pitch = makePitch(freqTable, clock);
  const a = analyse(framesOfCapture(run), { pitch });
  const onsets = a.voices.flatMap((x) => x.notes.map((n) => n.start));
  if (onsets.length === 0) throw new ConvertError("no notes found: the tune is silent, or all digi samples");
  const frames = a.tracks[0].voices[0].length;
  const timeline = rowStarts(segmentsOf(onsets), frames);
  const song = buildSong(a, { timeline, name, author, released });
  return { song, analysis: a, bytes: writeSng(song), notes: simplifications(a, song, run) };
}

export function writesOf(run) {
  return run.calls.flatMap((c) => c.writes.map(([cycle, chip, reg, value]) => [c.start + cycle, chip, reg, value]));
}

export function measure(run, played, freqTable) {
  const pitch = makePitch(freqTable, run.clock);
  const original = noteLists(framesOfCapture(run), pitch).slice(0, VOICES_A_CHIP);
  return closeness(original, noteLists(framesOfPlayer(played), pitch, original.length));
}

export function noteLists(chipFrames, pitch, voices) {
  return analyse(chipFrames, { pitch, voices }).voices.map((x) => x.notes.map((n) => ({ frame: n.start, note: n.note })));
}

export function closeness(a, b, tolerance = 2) {
  const diffs = [];
  a.forEach((va, v) => va.forEach((n) => (b[v] ?? []).forEach((m) => { if (m.note === n.note && Math.abs(m.frame - n.frame) <= LAG_MAX) diffs.push(m.frame - n.frame); })));
  diffs.sort((x, y) => x - y);
  const counts = new Map();
  diffs.forEach((d) => counts.set(d, (counts.get(d) ?? 0) + 1));
  let lag = 0, most = 0;
  counts.forEach((c, d) => { if (c > most) { most = c; lag = d; } });
  return a.map((va, v) => {
    const vb = b[v] ?? [];
    const taken = new Set();
    let matched = 0, offset = 0;
    for (const n of va) {
      let hit = -1;
      for (let j = 0; j < vb.length; j++) {
        if (taken.has(j) || vb[j].note !== n.note) continue;
        const d = Math.abs(vb[j].frame - lag - n.frame);
        if (d <= tolerance && (hit < 0 || d < Math.abs(vb[hit].frame - lag - n.frame))) hit = j;
      }
      if (hit >= 0) { taken.add(hit); matched++; offset += Math.abs(vb[hit].frame - lag - n.frame); }
    }
    const total = Math.max(va.length, vb.length);
    return { original: va.length, converted: vb.length, matched, score: total ? matched / total : 1, timing: matched ? offset / matched : 0, lag };
  });
}

export function frameMatch(a, b, lag, pitch) {
  return a.map((fa, v) => {
    const fb = b[v] ?? [];
    let same = 0, total = 0;
    for (let i = 0; i < fa.length; i++) {
      const g = fb[i + lag];
      if (!g) continue;
      const f = fa[i];
      const heard = audible(f) && (f.ctrl & CTRL.GATE || f.ctrl & CTRL.WAVES);
      if (!heard && !audible(g)) continue;
      total++;
      if ((f.ctrl & (CTRL.WAVES | CTRL.GATE)) === (g.ctrl & (CTRL.WAVES | CTRL.GATE)) && pitch(f.freq) === pitch(g.freq)) same++;
    }
    return total ? same / total : 1;
  });
}
