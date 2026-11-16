// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { crc32 } from "./sysex.js";

export const CLEAR = 255;
export const PAGE = 1248;
export const MAX_PACK = 65536;
export const NAME_MAX = 16;
export const PALETTE = [
  [0x00, 0x00, 0x00], [0xff, 0xff, 0xff], [0x68, 0x37, 0x2b], [0x70, 0xa4, 0xb2],
  [0x6f, 0x3d, 0x86], [0x58, 0x8d, 0x43], [0x35, 0x28, 0x79], [0xb8, 0xc7, 0x6f],
  [0x6f, 0x4f, 0x25], [0x43, 0x39, 0x00], [0x9a, 0x67, 0x59], [0x44, 0x44, 0x44],
  [0x6c, 0x6c, 0x6c], [0x9a, 0xd2, 0x84], [0x6c, 0x5e, 0xb5], [0x95, 0x95, 0x95],
];

export const MAGIC = "SIDOUART";
const VERSION = 1;
const SCENE_RMR = 1;
const HEAD = 32;
const SLOT_BYTES = 4;
const FRAME_BYTES = 12;
const CRC_BYTES = 4;
const NO_BASE = 0xff;
const CRC_RESIDUE = 0x2144df1c;
const RUN = 16;
const SEEK = 32;
const OPAQUE_ALPHA = 128;
const PATCH_OF_AN_IDENTICAL_POSE = [0, 0, 1, 1];

const POSES = ["guard", "guard 2", "raise", "chop", "block", "spin A", "spin B", "hit"];
const UNARMED = ["guard", "guard 2", "hit", "reach"];
const UNARMED_BASE = UNARMED.map((p) => (POSES.includes(p) ? POSES.indexOf(p) : null));
const FIGHTER = { w: 64, h: 124, gx: 2, anchor: [58, 112] };
const SCREEN_WIDE = { gx: 1, anchor: [0, 0] };

export const SLOTS = [
  { id: 1, file: "fighter-left-armed", name: "The left fighter, armed", ...FIGHTER, frames: POSES },
  { id: 2, file: "fighter-right-armed", name: "The right fighter, armed (drawn mirrored)", ...FIGHTER, frames: POSES },
  { id: 3, file: "fighter-left-unarmed", name: "The left fighter, unarmed", ...FIGHTER, frames: UNARMED, armed: 1 },
  { id: 4, file: "fighter-right-unarmed", name: "The right fighter, unarmed (drawn mirrored)", ...FIGHTER, frames: UNARMED, armed: 2 },
  { id: 5, file: "sword-flying", name: "The flying sword", w: 42, h: 84, gx: 2, anchor: [42, 42],
    frames: ["0°", "45°", "90°", "135°", "180°", "225°", "270°", "315°"] },
  { id: 6, file: "sword-stuck", name: "The sword stuck in the ground", w: 16, h: 44, gx: 2, anchor: [16, 10], frames: ["stuck"] },
  { id: 7, file: "sky", name: "The sky (moon, wisps, hills over the gradient)", w: 240, h: 124, ...SCREEN_WIDE,
    frames: ["closed filter", "half-open filter", "open filter"], sky: true },
  { id: 8, file: "arena", name: "The arena: columns, arch, torches' stands, flagstones", w: 240, h: 240, ...SCREEN_WIDE, frames: ["arena"] },
  { id: 9, file: "status-bar", name: "The status bar", w: 240, h: 16, ...SCREEN_WIDE, frames: ["status bar"] },
  { id: 10, file: "blade-empty", name: "A stamina blade, empty", w: 240, h: 16, ...SCREEN_WIDE, frames: ["empty"] },
  { id: 11, file: "blade-full", name: "A stamina blade, full", w: 240, h: 16, ...SCREEN_WIDE, frames: ["full"] },
  { id: 12, file: "blade-tip", name: "A blade's tip", w: 6, h: 5, ...SCREEN_WIDE, frames: ["tip"] },
];

export const FIRST_FRAME = SLOTS.reduce((at, s, k) => [...at, at[k] + s.frames.length], [0]);
export const FRAMES = FIRST_FRAME[SLOTS.length];
const TABLES = HEAD + SLOT_BYTES * SLOTS.length;
const FRAMES_END = TABLES + FRAME_BYTES * FRAMES;
export const SMALLEST_PACK = FRAMES_END + CRC_BYTES;

export class ArtError extends Error {}

const slotOfFrame = (i) => SLOTS[FIRST_FRAME.findLastIndex((f) => f <= i)];

function baseOf(i) {
  const s = slotOfFrame(i);
  if (!s.armed) return null;
  const k = UNARMED_BASE[i - FIRST_FRAME[s.id - 1]];
  return k === null ? null : FIRST_FRAME[s.armed - 1] + k;
}

export const withinAPage = (at, n) => Math.floor(at / PAGE) === Math.floor((at + Math.max(n, 1) - 1) / PAGE);

const rd16 = (b, at) => b[at] | (b[at + 1] << 8);

export function nameRefusal(name) {
  if (name.length < 1 || name.length > NAME_MAX) return `a pack's name is 1 to ${NAME_MAX} characters`;
  if (name[0] === " ") return "a pack's name does not start with a space";
  const bad = [...name].find((c) => c.charCodeAt(0) < 0x20 || c.charCodeAt(0) > 0x5f);
  return bad ? `"${bad}" is not in the FM-1's font (A-Z, 0-9, space and most punctuation)` : null;
}

export function headRefusal(bytes) {
  if (bytes.length < SMALLEST_PACK || bytes.length > MAX_PACK) return "too short, or over the store's 65,536 bytes a file";
  if (String.fromCharCode(...bytes.subarray(0, 8)) !== MAGIC) return "not a SIDOU art pack (.sidart)";
  if (bytes[8] !== VERSION) return "a newer art pack format than this page reads";
  if (bytes[9] !== SCENE_RMR) return "art for another scene than RED MOON RITE";
  const length = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(12, true);
  const slotsMatch = bytes[10] === SLOTS.length && bytes[11] === FRAMES && length === bytes.length &&
    SLOTS.every((s, k) => [s.id, s.w, s.h, s.frames.length].every((v, i) => bytes[HEAD + SLOT_BYTES * k + i] === v));
  if (!slotsMatch) return "its slots are not RED MOON RITE's: every image at its size and frame count";
  if (nameRefusal(packName(bytes)) || bytes.subarray(16 + packName(bytes).length, 32).some((b) => b)) return "its pack name is not 1 to 16 characters of the screen font";
  return null;
}

export function packRefusal(bytes) {
  return headRefusal(bytes) ?? (crc32(bytes) !== CRC_RESIDUE ? "its CRC does not match its bytes" : null);
}

export const packName = (bytes) => String.fromCharCode(...bytes.subarray(16, 32)).replace(/\0+$/, "");

export function decodePack(bytes) {
  const why = packRefusal(bytes);
  if (why) throw new ArtError(why);
  const frames = [];
  for (let i = 0; i < FRAMES; i++) {
    const s = slotOfFrame(i);
    const e = TABLES + FRAME_BYTES * i;
    const [x, y, w, h, , clear, , , base] = bytes.subarray(e, e + 9);
    const px = base === NO_BASE ? new Uint8Array(s.w * s.h).fill(CLEAR) : frames[base].slice();
    const table = rd16(bytes, e + 10);
    for (let r = 0; r < h; r++) {
      let at = rd16(bytes, table + 2 * r) + ((w - 1) >> 5);
      for (let xx = x; xx < x + w; at++) {
        const run = bytes[at];
        const c = (run & 15) === clear ? CLEAR : run & 15;
        px.fill(c, (y + r) * s.w + xx, (y + r) * s.w + xx + (run >> 4) + 1);
        xx += (run >> 4) + 1;
      }
    }
    frames.push(px);
  }
  return { name: packName(bytes), frames };
}

function bbox(w, h, keep) {
  let x0 = Infinity, y0 = Infinity, x1 = 0, y1 = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (keep(y * w + x)) {
        x0 = Math.min(x0, x); y0 = Math.min(y0, y);
        x1 = Math.max(x1, x + 1); y1 = Math.max(y1, y + 1);
      }
    }
  }
  return x0 === Infinity ? null : [x0, y0, x1 - x0, y1 - y0];
}

function rowRuns(row) {
  const runs = [];
  const split = row.length > SEEK;
  for (let x = 0; x < row.length;) {
    const c = row[x];
    const most = split ? Math.min(RUN, SEEK - (x % SEEK)) : RUN;
    let n = 1;
    while (n < most && x + n < row.length && row[x + n] === c) n++;
    runs.push([n, c]);
    x += n;
  }
  return runs;
}

function rectRows(px, fw, [x, y, w, h]) {
  const rows = [];
  for (let yy = y; yy < y + h; yy++) rows.push(rowRuns(px.subarray(yy * fw + x, yy * fw + x + w)));
  return rows;
}

function rowsCost(rows) {
  const seen = new Set();
  let runs = 0;
  for (const r of rows) {
    const key = r.join(";");
    if (!seen.has(key)) { seen.add(key); runs += r.length; }
  }
  return runs + 2 * rows.length;
}

function code(frames) {
  return frames.map((px, i) => {
    const s = slotOfFrame(i);
    const rect = bbox(s.w, s.h, (k) => px[k] !== CLEAR);
    if (!rect) throw new ArtError(`${s.name}, ${s.frames[i - FIRST_FRAME[s.id - 1]]}: the frame is empty (every pixel transparent)`);
    const whole = { rect, rows: rectRows(px, s.w, rect), base: null };
    const b = baseOf(i);
    if (b === null) return whole;
    const base = frames[b];
    const prect = bbox(s.w, s.h, (k) => px[k] !== base[k]) ?? PATCH_OF_AN_IDENTICAL_POSE;
    const patch = { rect: prect, rows: rectRows(px, s.w, prect), base: b };
    return rowsCost(patch.rows) < rowsCost(whole.rows) ? patch : whole;
  });
}

function clearCode(frames) {
  const all = new Set(frames.flatMap((px) => [...new Set(px)]));
  const shared = PALETTE.findIndex((_, c) => !all.has(c));
  return frames.map((px, i) => {
    if (shared >= 0) return shared;
    const mine = new Set(px);
    const own = PALETTE.findIndex((_, c) => !mine.has(c));
    if (own >= 0) return own;
    const s = slotOfFrame(i);
    throw new ArtError(`${s.name}, ${s.frames[i - FIRST_FRAME[s.id - 1]]}: uses all 16 colours; leave one out, a colour the frame does not use codes its transparency`);
  });
}

function packet(runs, clear) {
  const bytes = [];
  let pixel = 0;
  runs.forEach(([n], i) => {
    if (pixel !== 0 && pixel % SEEK === 0) bytes.push(i);
    pixel += n;
  });
  for (const [n, c] of runs) bytes.push(((n - 1) << 4) | (c === CLEAR ? clear : c));
  return bytes;
}

const compareBytes = (a, b) => {
  for (let i = 0; i < Math.min(a.length, b.length); i++) if (a[i] !== b[i]) return a[i] - b[i];
  return a.length - b.length;
};

class PackFile {
  constructor() {
    this.bytes = new Uint8Array(MAX_PACK);
    this.length = 0;
  }
  grow(n) {
    if (n > MAX_PACK - CRC_BYTES) throw new ArtError("the pack comes to over the store's 65,536 bytes a file: simplify the art (fewer colour changes along each row)");
    this.length = n;
  }
  padToPage() { this.grow(Math.ceil(this.length / PAGE) * PAGE); }
  append(bytes) {
    const at = this.length;
    this.grow(at + bytes.length);
    this.bytes.set(bytes, at);
    return at;
  }
  matches(at, bytes, from = 0, n = bytes.length) {
    for (let i = from; i < n; i++) if (this.bytes[at + i - from] !== bytes[i]) return false;
    return true;
  }
  place(from, bytes) {
    const n = bytes.length;
    for (let at = from; at + n <= this.length; at++) {
      if (this.bytes[at] === bytes[0] && this.matches(at, bytes) && withinAPage(at, n)) return at;
    }
    const tail = this.length - from;
    for (let k = Math.min(n, tail + 1) - 1; k >= 1; k--) {
      const at = this.length - k;
      if (this.matches(at, bytes, 0, k) && withinAPage(at, n)) {
        this.append(bytes.slice(k));
        return at;
      }
    }
    if (!withinAPage(this.length, n)) this.padToPage();
    return this.append(bytes);
  }
  put16(at, v) { this.bytes[at] = v & 255; this.bytes[at + 1] = v >> 8; }
}

export function encodePack(frames, name, { skies } = {}) {
  const why = nameRefusal(name);
  if (why) throw new ArtError(why);
  if (frames.length !== FRAMES) throw new ArtError(`a pack has ${FRAMES} frames, not ${frames.length}`);
  const sources = frames.map((px, i) => {
    const s = slotOfFrame(i);
    if (px.length !== s.w * s.h) throw new ArtError(`${s.name}: a frame of ${s.w} × ${s.h}, not ${px.length} pixels`);
    if (!s.sky) return px;
    const sky = skies[i - FIRST_FRAME[s.id - 1]];
    return px.map((p, k) => (p === sky[k] ? CLEAR : p));
  });
  const clears = clearCode(sources);
  const coded = code(sources);
  const file = new PackFile();
  file.grow(FRAMES_END);
  file.bytes.set([...MAGIC].map((c) => c.charCodeAt(0)), 0);
  file.bytes.set([VERSION, SCENE_RMR, SLOTS.length, FRAMES], 8);
  file.bytes.set([...name].map((c) => c.charCodeAt(0)), 16);
  SLOTS.forEach((s, k) => file.bytes.set([s.id, s.w, s.h, s.frames.length], HEAD + SLOT_BYTES * k));
  const rowTables = coded.map((c) => {
    const n = 2 * c.rows.length;
    if (!withinAPage(file.length, n)) file.padToPage();
    const at = file.length;
    file.grow(at + n);
    return at;
  });
  const data = file.length;
  const packets = coded.map((c, i) => c.rows.map((r) => packet(r, clears[i])));
  const unique = [...new Map(packets.flat().map((p) => [p.join(","), p])).values()]
    .sort((a, b) => b.length - a.length || compareBytes(a, b));
  const at = new Map(unique.map((p) => [p.join(","), file.place(data, p)]));
  coded.forEach((c, i) => {
    const s = slotOfFrame(i);
    const [x, y, w, h] = c.rect;
    const e = TABLES + FRAME_BYTES * i;
    file.bytes.set([x, y, w, h, s.gx, clears[i], s.anchor[0], s.anchor[1], c.base ?? NO_BASE, 0], e);
    file.put16(e + 10, rowTables[i]);
    packets[i].forEach((p, r) => file.put16(rowTables[i] + 2 * r, at.get(p.join(","))));
  });
  const length = file.length + CRC_BYTES;
  new DataView(file.bytes.buffer).setUint32(12, length, true);
  const out = new Uint8Array(length);
  out.set(file.bytes.subarray(0, file.length));
  new DataView(out.buffer).setUint32(file.length, crc32(out.subarray(0, file.length)), true);
  return out;
}

export function nearest(r, g, b) {
  let best = 0, far = Infinity;
  PALETTE.forEach(([pr, pg, pb], c) => {
    const d = (r - pr) ** 2 + (g - pg) ** 2 + (b - pb) ** 2;
    if (d < far) { far = d; best = c; }
  });
  return best;
}

export function toPalette(rgba) {
  const px = new Uint8Array(rgba.length / 4);
  let moved = 0, clear = 0;
  for (let k = 0; k < px.length; k++) {
    const [r, g, b, a] = rgba.subarray(4 * k, 4 * k + 4);
    if (a < OPAQUE_ALPHA) { px[k] = CLEAR; clear++; continue; }
    const c = nearest(r, g, b);
    const [pr, pg, pb] = PALETTE[c];
    if (a !== 255 || r !== pr || g !== pg || b !== pb) moved++;
    px[k] = c;
  }
  return { px, moved, clear };
}

export function toRgba(px) {
  const rgba = new Uint8ClampedArray(px.length * 4);
  px.forEach((c, k) => { if (c !== CLEAR) rgba.set([...PALETTE[c], 255], 4 * k); });
  return rgba;
}

export function sheetSize(slot) {
  return { w: slot.w * slot.frames.length, h: slot.h };
}

export function sheetRefusal(slot, w, h) {
  const want = sheetSize(slot);
  if (w === want.w && h === want.h) return null;
  const n = slot.frames.length;
  const layout = n > 1 ? `its ${n} frames of ${slot.w} × ${slot.h} side by side in a row` : `one frame of ${slot.w} × ${slot.h}`;
  const wide = slot.gx > 1 && w === want.w * slot.gx && h === want.h
    ? ` It looks drawn at the screen's width: this slot is stored at 1/${slot.gx} width (each stored pixel is ${slot.gx} on the screen), so halve its width.` : "";
  return `${slot.name} takes a ${want.w} × ${want.h} PNG, ${layout}; this one is ${w} × ${h}.${wide}`;
}

export function framesFromSheet(slot, px) {
  const { w } = sheetSize(slot);
  return slot.frames.map((_, k) => {
    const f = new Uint8Array(slot.w * slot.h);
    for (let y = 0; y < slot.h; y++) f.set(px.subarray(y * w + k * slot.w, y * w + (k + 1) * slot.w), y * slot.w);
    return f;
  });
}

export function sheetFromFrames(slot, frames) {
  const { w, h } = sheetSize(slot);
  const px = new Uint8Array(w * h);
  frames.forEach((f, k) => {
    for (let y = 0; y < slot.h; y++) px.set(f.subarray(y * slot.w, (y + 1) * slot.w), y * w + k * slot.w);
  });
  return px;
}

export const slotFrames = (frames, slot) => frames.slice(FIRST_FRAME[slot.id - 1], FIRST_FRAME[slot.id]);
