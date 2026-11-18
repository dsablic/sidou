// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
const PATCH_FORMAT = "sidou-patch/1";
const NAME_LENGTH = 12;
const TABLE_STEPS_MAX = 16;
const TABLE = { WAVE: 0, PULSE: 1, FILTER: 2 };
const WAVES = [[0x10, "triangle", "TRI"], [0x20, "saw", "SAW"], [0x40, "pulse", "PUL"], [0x80, "noise", "NOI"]];
const FILTER_MODES = [[0x10, "low"], [0x20, "band"], [0x40, "high"]];
const CTRL = { GATE: 0x01, SYNC: 0x02, RING: 0x04, TEST: 0x08, PULSE: 0x40, WAVES: 0xf0 };
const ROW = { SAME: 0x00, FIRST_WAVE: 0x10, LAST_WAVE: 0xdf, JUMP: 0xff, STOP: 0x00, SET: 0x80, SET_VALUE: 0x0f };
const NOTE = { KEEP: 0x80, DOWN_FIRST: 0x60, DOWN_FROM: 0x80, FIXED_FROM: 0x80 };
const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const PULSE_UNITS = 4096, PULSE_SPEED_SIGNED = 0x80, PULSE_SPEED_WRAP = 0x100;
const PWM_MIN_PERCENT = 0.5, PWM_MAX_PERCENT = 50, PWM_MAX_HZ = 12.75, FRAME_HZ = 50;
const FILTER_VOICES = 0x07, FILTER_RESONANCE_SHIFT = 4, CUTOFF_SHIFT = 3;
const NIBBLE = 0x0f, NIBBLE_BITS = 4;
const ALL_VOICES = [1, 2, 3];
const KEY_NOTE = 0;

const names = (bits, table) => table.filter(([bit]) => bits & bit).map(([, name]) => name);
const waveOf = (ctrl) => names(ctrl & CTRL.WAVES, WAVES).join("+") || "none";
const shortWaveOf = (ctrl) => WAVES.filter(([bit]) => ctrl & bit).map(([, , short]) => short).join("+") || "NONE";
const percent = (units) => Math.round((units / PULSE_UNITS) * 10000) / 100;
const signed = (byte) => (byte >= PULSE_SPEED_SIGNED ? byte - PULSE_SPEED_WRAP : byte);

const noteOf = (raw) => {
  if (raw === NOTE.KEEP) return "hold";
  if (raw > NOTE.FIXED_FROM) {
    const n = raw - NOTE.FIXED_FROM;
    return `${NOTE_NAMES[n % NOTE_NAMES.length]}${Math.floor(n / NOTE_NAMES.length)}`;
  }
  return raw >= NOTE.DOWN_FIRST ? raw - NOTE.DOWN_FROM : raw;
};

const rowsFrom = (tables, t, pointer, until) => {
  const rows = [];
  for (let at = pointer - 1; at >= 0 && at < tables.left[t].length; at++) {
    const row = { left: tables.left[t][at], right: tables.right[t][at], at: at + 1 };
    if (until(row)) return { rows, end: row };
    rows.push(row);
  }
  return { rows, end: null };
};

function waveSteps(song, ins) {
  if (!ins.wave) return { steps: [{ ctrl: ins.first & ~CTRL.GATE, note: KEY_NOTE }], loop: 0 };
  const { rows, end } = rowsFrom(song.tables, TABLE.WAVE, ins.wave, (r) => r.left === ROW.JUMP);
  let ctrl = 0;
  const steps = rows.flatMap((r) => {
    if (r.left !== ROW.SAME && (r.left < ROW.FIRST_WAVE || r.left > ROW.LAST_WAVE)) return [];
    if (r.left !== ROW.SAME) ctrl = r.left & ~CTRL.GATE;
    return [{ ctrl, note: r.right }];
  });
  const target = end && end.right !== ROW.STOP ? end.right - ins.wave + 1 : 0;
  return { steps, loop: target >= 1 && target <= steps.length ? target : 0 };
}

const sameStep = (a, b) => a.ctrl === b.ctrl && a.note === b.note;

function trimmed(steps, loop, simplified) {
  const out = steps.slice();
  if (loop === 0) while (out.length > 1 && sameStep(out.at(-1), out.at(-2))) out.pop();
  if (out.length > TABLE_STEPS_MAX) {
    out.length = TABLE_STEPS_MAX;
    simplified.push(`the wave table is cut to LAB's ${TABLE_STEPS_MAX} steps`);
    return { steps: out, loop: loop > TABLE_STEPS_MAX ? 0 : loop };
  }
  return { steps: out, loop };
}

const needsTable = (steps, loop) => loop > 0 || steps.some((s) => s.ctrl !== steps[0].ctrl || (s.note !== KEY_NOTE && s.note !== NOTE.KEEP));

const stepJson = (s) => ({
  wave: waveOf(s.ctrl),
  ...(s.ctrl & CTRL.TEST ? { test: true } : {}),
  ...(s.ctrl & CTRL.RING ? { ring: true } : {}),
  ...(s.ctrl & CTRL.SYNC ? { sync: true } : {}),
  note: noteOf(s.note),
});

function pulseOf(song, ins) {
  const { rows } = rowsFrom(song.tables, TABLE.PULSE, ins.pulse, (r) => r.left === ROW.JUMP);
  const set = rows[0]?.left >= ROW.SET ? rows[0] : null;
  const start = set ? ((set.left & ROW.SET_VALUE) << 8) | set.right : PULSE_UNITS / 2;
  const runs = rows.slice(set ? 1 : 0).filter((r) => r.left < ROW.SET).map((r) => ({ time: r.left, travel: r.left * signed(r.right) }));
  return { start, runs };
}

function pwmOf({ start, runs }) {
  if (runs.length === 0) return { width: percent(start), pwm: null };
  const depth = percent(Math.max(...runs.map((r) => Math.abs(r.travel))) / 2);
  const frames = runs.length > 1 ? runs.reduce((sum, r) => sum + r.time, 0) : runs[0].time * 2;
  const width = Math.max(0, Math.min(100, percent(start + runs[0].travel / 2)));
  if (depth < PWM_MIN_PERCENT) return { width: percent(start), pwm: null };
  return { width, pwm: { depth: Math.min(PWM_MAX_PERCENT, depth), rate_hz: Math.min(PWM_MAX_HZ, Math.round((FRAME_HZ / frames) * 100) / 100) } };
}

function filterOf(song, ins, cutoffHz) {
  if (!ins.filter) return null;
  const { rows } = rowsFrom(song.tables, TABLE.FILTER, ins.filter, (r) => r.left === ROW.JUMP);
  const set = rows.find((r) => r.left >= ROW.SET);
  const cut = rows.find((r) => r.left === ROW.SAME);
  if (!set || !(set.left & 0x70) || !(set.right & FILTER_VOICES)) return null;
  return {
    voices: set.right & FILTER_VOICES,
    json: { mode: names(set.left, FILTER_MODES).join("+"), cutoff_hz: cutoffHz((cut?.right ?? 0) << CUTOFF_SHIFT), resonance: set.right >> FILTER_RESONANCE_SHIFT, voices: ALL_VOICES },
  };
}

const distinct = (list) => list.filter((x, i) => list.indexOf(x) === i);

function describe(steps, loop, pwm, filter) {
  const waves = distinct(steps.map((s) => waveOf(s.ctrl))).join(" then ");
  const arpeggio = loop > 0 && new Set(steps.slice(loop - 1).map((s) => s.note)).size > 1;
  const moves = !arpeggio && steps.some((s) => s.note !== KEY_NOTE && s.note !== NOTE.KEEP);
  return [waves, pwm && "pulse sweep", filter && "filter", arpeggio && "arpeggio", moves && "pitch steps"].filter(Boolean).join(", ");
}

function patchOf(song, ins, number, cutoffHz) {
  const simplified = [];
  const { steps: all, loop: allLoop } = waveSteps(song, ins);
  const { steps, loop } = trimmed(all, allLoop, simplified);
  const last = steps.at(-1);
  const pulsed = steps.some((s) => s.ctrl & CTRL.PULSE);
  const { width, pwm } = pulsed && ins.pulse ? pwmOf(pulseOf(song, ins)) : { width: null, pwm: null };
  const filter = filterOf(song, ins, cutoffHz);
  if (pwm) simplified.push("the pulse sweep is LAB's PWM, a triangle at the sweep's rate and depth");
  if (filter) {
    simplified.push("the filter is its setting at the note's start: the tune's moves of it are not kept");
    if (filter.voices !== FILTER_VOICES) simplified.push("LAB filters every voice of a sound; the tune filtered only some");
  }
  const patch = {
    format: PATCH_FORMAT,
    name: `${ins.voice}.${number} ${steps.map((s) => shortWaveOf(s.ctrl)).at(-1)}`.slice(0, NAME_LENGTH).trim(),
    wave: waveOf(last.ctrl),
    ...(last.ctrl & CTRL.RING ? { ring: true } : {}),
    ...(last.ctrl & CTRL.SYNC ? { sync: true } : {}),
    ...(width === null ? {} : { pulse_width: width }),
    envelope: { attack: ins.ad >> NIBBLE_BITS, decay: ins.ad & NIBBLE, sustain: ins.sr >> NIBBLE_BITS, release: ins.sr & NIBBLE },
    filter: filter ? filter.json : { voices: [] },
    ...(pwm ? { pwm } : {}),
    ...(needsTable(steps, loop) ? { table: { loop, frames: 1, steps: steps.map(stepJson) } } : {}),
  };
  return { patch, simplified, about: describe(steps, loop, pwm, filter) };
}

export function soundsOf(song, { cutoffHz }) {
  return song.instruments.map((ins, k) => ({
    index: k + 1,
    voice: ins.voice,
    uses: ins.uses,
    ...patchOf(song, ins, k + 1, cutoffHz),
  }));
}

export const textOf = (patch) => `${JSON.stringify(patch, null, 2)}\n`;

export function tuneSounds(song, patcher) {
  return soundsOf(song, { cutoffHz: (fc) => patcher.cutoffHz(fc) }).map((s) => {
    const text = textOf(s.patch);
    const checked = patcher.check(text);
    return {
      name: s.patch.name,
      about: `${s.voice}, ${s.uses} note${s.uses === 1 ? "" : "s"}: ${s.about}`,
      simplified: s.simplified,
      text,
      refused: checked.ok ? "" : checked.error,
    };
  });
}
