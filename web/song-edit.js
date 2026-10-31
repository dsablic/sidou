// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { EditError } from "./core.js";

const HEX = "0123456789ABCDEF";
const NOTE_NAMES = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];
export const hex2 = (v) => HEX[(v >> 4) & 15] + HEX[v & 15];

export const FIRSTNOTE = 0x60, LASTNOTE = 0xbc, REST = 0xbd, KEYOFF = 0xbe, KEYON = 0xbf;
export const REPEAT = 0xd0, TRANSDOWN = 0xe0, TRANSUP = 0xf0;

export const INSTR_PARAMS = ["ATTACK/DECAY", "SUSTAIN/RELEASE", "WAVE PTR", "PULSE PTR", "FILTER PTR",
  "VIBRATO", "VIB DELAY", "HR/GATE", "1ST WAVE"];
export const TABLE_NAMES = ["WAVE", "PULSE", "FILTER", "SPEED"];
export const COMMANDS = ["", "PORTAMENTO UP", "PORTAMENTO DOWN", "TONEPORTAMENTO", "VIBRATO", "SET ATTACK/DECAY",
  "SET SUSTAIN/RELEASE", "SET WAVEFORM", "WAVETABLE POINTER", "PULSETABLE POINTER", "FILTERTABLE POINTER",
  "FILTER CONTROL", "FILTER CUTOFF", "MASTER VOLUME", "FUNKTEMPO", "SET TEMPO"];

export function cellText(c) {
  const n = c[0];
  let s;
  if (n >= FIRSTNOTE && n <= LASTNOTE) {
    const v = n - FIRSTNOTE;
    s = NOTE_NAMES[v % 12] + String.fromCharCode(48 + Math.floor(v / 12));
  } else {
    const ch = n === KEYOFF ? "=" : n === KEYON ? "+" : "-";
    s = ch + ch + ch;
  }
  s += c[1] ? hex2(c[1]) : "..";
  s += c[2] ? HEX[c[2] & 15] + hex2(c[3]) : "...";
  return s;
}

export const noteText = (n) => cellText([n, 0, 0, 0]).slice(0, 3);
export function parseNote(text) {
  const t = text.trim().toUpperCase();
  if (t === "" || /^-+$|^\.+$/.test(t)) return REST;
  if (/^=+$|^OFF$/.test(t)) return KEYOFF;
  if (/^\++$|^ON$/.test(t)) return KEYON;
  const m = /^([A-G])([#B-]?)(\d)$/.exec(t.replace(/^([A-G])(\d)$/, "$1-$2"));
  if (!m) return null;
  let k = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }[m[1]] + (m[2] === "#" ? 1 : m[2] === "B" ? -1 : 0);
  const v = Number(m[3]) * 12 + k;
  return v < 0 || v > LASTNOTE - FIRSTNOTE ? null : FIRSTNOTE + v;
}

export function orderText(e) {
  if (e === TRANSDOWN) return "-10";
  if (e < REPEAT) return hex2(e);
  if (e < TRANSDOWN) return `R${HEX[e & 15]}`;
  if (e < TRANSUP) return `-${HEX[(TRANSUP - e) & 15]}`;
  return `+${HEX[(e - TRANSUP) & 15]}`;
}
export function parseOrder(text) {
  const t = text.trim().toUpperCase();
  let m;
  if ((m = /^R([0-9A-F])$/.exec(t))) return REPEAT + parseInt(m[1], 16);
  if (t === "-10") return TRANSDOWN;
  if ((m = /^\+([0-9A-F])$/.exec(t))) return TRANSUP + parseInt(m[1], 16);
  if ((m = /^-([0-9A-F])$/.exec(t))) return (TRANSUP - parseInt(m[1], 16)) & 0xff;
  if ((m = /^[0-9A-F]{1,2}$/.exec(t))) {
    const v = parseInt(t, 16);
    return v < REPEAT ? v : null;
  }
  return null;
}
export function orderKind(e) {
  if (e < REPEAT) return `PATTERN ${hex2(e)}`;
  if (e < TRANSDOWN) return `REPEAT ${e & 15 ? e & 15 : 16}`;
  return `TRANSPOSE ${orderText(e)}`;
}

export function tableMeaning(table, left, right) {
  let what;
  if (table !== 3 && left === 0xff) what = right ? "JUMP " : "STOP ";
  else if (table === 0) what = left <= 0x0f ? "DELAY " : left >= 0xf0 ? "COMMAND " : "WAVE ";
  else if (table === 1) what = left & 0x80 ? "SET " : "SLIDE ";
  else if (table === 2) what = !left ? "CUTOFF " : left & 0x80 ? "CTRL " : "SLIDE ";
  else what = left & 0x80 ? "CALC " : "SPEED ";
  if (table !== 3 && left === 0xff) return what + hex2(right);
  let s = `${what}${hex2(left)} `;
  if (table === 0 && left >= 0x10 && left < 0xf0) {
    if (right > 0x80 && (right & 0x7f) < 96) {
      const note = right & 0x7f;
      return s + NOTE_NAMES[note % 12] + String.fromCharCode(48 + Math.floor(note / 12));
    }
    if (right === 0x80) return `${s}KEEP`;
    s += right > 0x80 ? "ABS " : "REL ";
    if (right > 0x80) right &= 0x7f;
  }
  return s + hex2(right);
}

export const parseHex = (text, max = 0xff) => {
  const t = text.trim();
  if (!/^[0-9a-fA-F]{1,2}$/.test(t)) return null;
  const v = parseInt(t, 16);
  return v > max ? null : v;
};

export class SongSession {
  constructor(core, { limit = 200 } = {}) {
    this.core = core;
    this.limit = limit;
    this.undoStack = [];
    this.redoStack = [];
    this.revision = 0;
    this.savedRevision = 0;
    this.info = null;
    this.source = null;
    this.cache = null;
  }

  open(bytes, source = {}) {
    this.info = this.core.read(bytes);
    this.source = source;
    this.undoStack = [];
    this.redoStack = [];
    this.revision = 0;
    this.savedRevision = 0;
    this.cache = null;
    return this.info;
  }

  get dirty() { return this.revision !== this.savedRevision; }
  get canUndo() { return this.undoStack.length > 0; }
  get canRedo() { return this.redoStack.length > 0; }

  get song() {
    if (!this.cache) this.cache = this.core.song();
    return this.cache;
  }

  apply(label, fn) {
    const before = this.core.snapshot();
    let result;
    try {
      result = fn(this.core);
    } catch (e) {
      this.core.restore(before);
      this.cache = null;
      throw e;
    }
    this.undoStack.push({ label, snap: before });
    if (this.undoStack.length > this.limit) this.undoStack.shift();
    this.redoStack = [];
    this.changed();
    return result;
  }

  changed() {
    this.revision++;
    this.cache = null;
  }

  undo() {
    const step = this.undoStack.pop();
    if (!step) return null;
    this.redoStack.push({ label: step.label, snap: this.core.snapshot() });
    this.core.restore(step.snap);
    this.changed();
    return step.label;
  }

  redo() {
    const step = this.redoStack.pop();
    if (!step) return null;
    this.undoStack.push({ label: step.label, snap: this.core.snapshot() });
    this.core.restore(step.snap);
    this.changed();
    return step.label;
  }

  bytes() { return this.core.write().bytes; }

  async saveToFm1(lib, name, { confirmReplace = async () => false, ...opts } = {}) {
    const bytes = this.bytes();
    let replace = false;
    if (lib.status(name, bytes) === "replace") {
      replace = await confirmReplace(name);
      if (!replace) return "kept";
    }
    const r = await lib.send(name, bytes, { type: 1, replace, ...opts });
    this.savedRevision = this.revision;
    if (this.source) this.source = { ...this.source, name, fm1: true, current: false };
    return r;
  }

  async loadNow(client, opts = {}) {
    await client.loadNow(this.bytes(), opts);
  }
}

export { EditError };
