// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { crc32 } from "./sysex.js";

export { crc32 };
export const MAGIC = [0x7d, 0x46, 0x4d, 0x31, 0x4c];
export const VERSION = 1;
export const PAYLOAD_MAX = 48;
export const START = 0x01, HEARTBEAT = 0x02, STOP = 0x03, KEYFRAME = 0x04, FONT = 0x05, PALETTE = 0x06, PANEL = 0x07;
export const HELLO = 0x41, BEAT = 0x42, STOPPED = 0x43, COLOURS = 0x44, GLYPHS = 0x45, CELLS = 0x46, PIXELS = 0x47;
export const HEARTBEAT_MS = 2000;
export const PANEL_REPEAT_MS = 200;
export const STEPS_MS = 25;
export const KEYFRAME_GAP_MS = 1000;
export const SILENT_MS = 5000;

export const BUTTONS = ["OCT-", "OCT+", "FX", "SCL", "ENV", "LFO", "EDIT", "GLO", "HOME", "SAVE", "ARP", "SEQ", "PLAY", "REC"];
export const ENCODERS = ["SELECT", "ALGORITHM", "K1", "K2", "K3", "K4", "PRESETS"];

export function pack(payload) {
  const out = [];
  for (let i = 0; i < payload.length; i += 7) {
    const group = payload.slice(i, i + 7);
    let mask = 0;
    group.forEach((b, k) => { mask |= (b >> 7) << k; });
    out.push(mask, ...Array.from(group, (b) => b & 0x7f));
  }
  return Uint8Array.from(out);
}

export function unpack(packed) {
  const out = [];
  for (let i = 0; i < packed.length; i += 8) {
    const mask = packed[i];
    const low = packed.slice(i + 1, i + 8);
    if (!low.length || mask >> low.length || mask & 0x80 || low.some((b) => b & 0x80)) return null;
    low.forEach((b, k) => out.push(b | (((mask >> k) & 1) << 7)));
  }
  return Uint8Array.from(out);
}

export function frame(cmd, payload = []) {
  const body = [cmd, ...pack(Uint8Array.from(payload))];
  const sum = body.reduce((s, b) => s + b, 0);
  return Uint8Array.from([0xf0, ...MAGIC, ...body, (128 - (sum & 0x7f)) & 0x7f, 0xf7]);
}

export const isLink = (f) => f.length >= 6 && f[0] === 0xf0 && MAGIC.every((b, i) => f[i + 1] === b);

export function parse(f) {
  if (f.length < 9 || !isLink(f) || f[f.length - 1] !== 0xf7) return null;
  const body = f.subarray ? f.subarray(6, f.length - 1) : f.slice(6, -1);
  if (body.reduce((s, b) => s + b, 0) & 0x7f) return null;
  const payload = unpack(body.slice(1, body.length - 1));
  return payload ? { cmd: body[0], payload } : null;
}

export const startFrame = () => frame(START, [VERSION]);
export const heartbeatFrame = () => frame(HEARTBEAT);
export const stopFrame = () => frame(STOP);
export const keyframeFrame = () => frame(KEYFRAME);
export const fontFrame = () => frame(FONT);
export const paletteFrame = () => frame(PALETTE);

export function panelFrame(held, pressed, steps = []) {
  let n = steps.length;
  while (n && !steps[n - 1]) n--;
  return frame(PANEL, [held & 255, (held >> 8) & 255, pressed & 255, (pressed >> 8) & 255, ...steps.slice(0, n).map((s) => s & 255)]);
}

export class Splitter {
  constructor() { this.frame = []; this.open = false; }
  push(bytes) {
    const out = [];
    for (const b of bytes) {
      if (b >= 0xf8) continue;
      if (b === 0xf0) { this.frame = []; this.open = true; }
      else if (!this.open) continue;
      else if (b & 0x80 && b !== 0xf7) { this.open = false; continue; }
      this.frame.push(b);
      if (b === 0xf7) { this.open = false; out.push(Uint8Array.from(this.frame)); }
    }
    return out;
  }
}

export function parseHello(p) {
  if (p.length < 22) return null;
  const le = (i) => (p[i] | (p[i + 1] << 8) | (p[i + 2] << 16) | (p[i + 3] << 24)) >>> 0;
  return {
    version: p[0], features: p[1], cols: p[2], rows: p[3], cellW: p[4], cellH: p[5], colours: p[6],
    fontFirst: p[7], fontLast: p[8], pieceFirst: p[9], pieces: p[10], timeoutMs: p[11] * 100,
    buttons: p[12], encoders: p[13], fontCrc: le(14), paletteCrc: le(18),
    name: String.fromCharCode(...p.slice(22)).replace(/[^\x20-\x7e]/g, ""),
  };
}

export async function loadViewer(source) {
  const bytes = source instanceof ArrayBuffer || ArrayBuffer.isView(source) ? source : await (await fetch(source)).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Viewer(instance.exports);
}

export const WIDTH = 240, HEIGHT = 240;

export class Viewer {
  constructor(x) {
    this.x = x;
    x.lv_init();
  }
  get mem() { return this.x.memory.buffer; }
  apply(cmd, payload) {
    if (payload.length > PAYLOAD_MAX) return false;
    new Uint8Array(this.mem, this.x.lv_in(), payload.length).set(payload);
    return this.x.lv_frame(cmd, payload.length) === 1;
  }
  reset() { this.x.lv_reset(); }
  render() { return new Uint8ClampedArray(this.mem, this.x.lv_render(), WIDTH * HEIGHT * 4); }
  rgb565() { return new Uint16Array(this.mem, this.x.lv_fb565(), WIDTH * HEIGHT); }
  fontRows(first = 0, last = this.x.lv_font_codes() - 1) {
    if (last >= this.x.lv_font_codes() || first > last) return null;
    return new Uint8Array(this.mem, this.x.lv_font() + first * 8, (last - first + 1) * 8);
  }
  palette() { return new Uint8Array(this.mem, this.x.lv_palette(), 32); }
  fontCrc(first, last) { const r = this.fontRows(first, last); return r ? crc32(r) : null; }
  paletteCrc() { return crc32(this.palette()); }
  setFont(rows, first = 0) { this.fontRows(first, first + rows.length / 8 - 1)?.set(rows); this.x.lv_touch(); }
  setPalette(bytes) { this.palette().set(bytes); this.x.lv_touch(); }
}

const hexOf = (b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const bytesOf = (h) => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));

export class LinkClient {
  constructor({ send, viewer = null, clock = globalThis, cache = null, onChange = () => {}, onPicture = () => {} } = {}) {
    this.send = send;
    this.viewer = viewer;
    this.clock = clock;
    this.now = clock.now ? () => clock.now() : () => Date.now();
    this.cache = cache;
    this.onChange = onChange;
    this.onPicture = onPicture;
    this.state = "off";
    this.hello = null;
    this.splitter = new Splitter();
    this.stats = { frames: 0, bytes: 0, rejected: 0, keyframes: 0 };
    this.held = 0;
    this.pressed = 0;
    this.steps = [];
    this.timers = {};
    this.heard = 0;
    this.keyframeAt = -Infinity;
    this.hiddenStop = false;
  }

  get on() { return this.state === "on"; }

  set(state, why = "") {
    this.state = state;
    this.why = why;
    this.onChange(state, why);
  }

  #out(bytes) { this.send(bytes); }

  #every(name, ms, fn) {
    if (!this.timers[name]) this.timers[name] = this.clock.setInterval(fn, ms);
  }
  #cancel(name) {
    if (this.timers[name] == null) return;
    this.clock.clearInterval(this.timers[name]);
    this.clock.clearTimeout(this.timers[name]);
    delete this.timers[name];
  }

  start() {
    this.hello = null;
    this.held = this.pressed = 0;
    this.steps = [];
    this.#cancel("panel");
    this.#cancel("steps");
    this.hiddenStop = false;
    this.heard = this.now();
    this.set("starting");
    this.#out(startFrame());
    this.#every("beat", HEARTBEAT_MS, () => this.#beat());
  }

  #beat() {
    if (this.state !== "starting" && this.state !== "on" && this.state !== "silent") return;
    this.#out(heartbeatFrame());
    if (this.now() - this.heard > SILENT_MS && this.state !== "silent") this.set("silent", "no answer from the FM-1");
  }

  stop(why = "") {
    const was = this.state;
    this.#end();
    if (was === "starting" || was === "on" || was === "silent") this.#out(stopFrame());
    if (was !== "off") this.set("stopped", why);
  }

  #end() {
    for (const name of Object.keys(this.timers)) this.#cancel(name);
    this.held = this.pressed = 0;
    this.steps = [];
  }

  keyframe() {
    if (!this.on) return;
    this.keyframeAt = this.now();
    this.stats.keyframes++;
    this.#out(keyframeFrame());
  }

  receive(bytes) {
    for (const f of this.splitter.push(bytes)) {
      if (!isLink(f)) continue;
      const p = parse(f);
      if (!p) { this.stats.rejected++; this.#lost(); continue; }
      this.#take(p.cmd, p.payload);
    }
  }

  #lost() {
    if (this.on && this.now() - this.keyframeAt >= KEYFRAME_GAP_MS) this.keyframe();
  }

  #take(cmd, p) {
    const live = this.state === "starting" || this.state === "on" || this.state === "silent";
    if (!live) return;
    this.heard = this.now();
    if (cmd === HELLO) {
      const h = parseHello(p);
      if (!h) return;
      if (this.viewer && !this.viewer.apply(HELLO, p)) {
        this.#end();
        this.#out(stopFrame());
        this.set("stopped", `this viewer cannot draw a ${h.cols} x ${h.rows} screen of ${h.cellW} x ${h.cellH} cells`);
        return;
      }
      this.hello = h;
      this.set("on");
      this.#fetch();
      this.onPicture();
      return;
    }
    if (cmd === STOPPED) {
      this.#end();
      this.hello = null;
      this.set("ended", "the FM-1 ended the session");
      return;
    }
    if (this.state === "silent") this.set("on");
    if (cmd === BEAT || !this.hello) return;
    this.stats.frames++;
    this.stats.bytes += p.length;
    if (!this.viewer) return;
    if (!this.viewer.apply(cmd, p)) {
      this.stats.rejected++;
      this.#lost();
      return;
    }
    if (cmd === GLYPHS || cmd === COLOURS) this.#keep(cmd);
    this.onPicture();
  }

  #fetch() {
    if (!this.viewer) return;
    const h = this.hello;
    if (this.viewer.fontCrc(h.fontFirst, h.fontLast) !== h.fontCrc) {
      const rows = this.#cached(`font-${h.fontCrc}`);
      if (rows && rows.length === (h.fontLast - h.fontFirst + 1) * 8 && crc32(rows) === h.fontCrc) this.viewer.setFont(rows, h.fontFirst);
      else this.#out(fontFrame());
    }
    if (this.viewer.paletteCrc() !== h.paletteCrc) {
      const pal = this.#cached(`palette-${h.paletteCrc}`);
      if (pal && pal.length === 32 && crc32(pal) === h.paletteCrc) this.viewer.setPalette(pal);
      else this.#out(paletteFrame());
    }
  }

  #cached(key) {
    try {
      const v = this.cache?.getItem?.(`sidou-link-${key}`) ?? this.cache?.get?.(`sidou-link-${key}`);
      return v ? bytesOf(v) : null;
    } catch {
      return null;
    }
  }

  #keep(cmd) {
    const h = this.hello;
    const [key, bytes, want] = cmd === GLYPHS
      ? [`font-${h.fontCrc}`, this.viewer.fontRows(h.fontFirst, h.fontLast), h.fontCrc]
      : [`palette-${h.paletteCrc}`, this.viewer.palette(), h.paletteCrc];
    if (!bytes || crc32(bytes) !== want) return;
    try {
      if (this.cache?.setItem) this.cache.setItem(`sidou-link-${key}`, hexOf(bytes));
      else this.cache?.set?.(`sidou-link-${key}`, hexOf(bytes));
    } catch {}
  }

  #panel() {
    if (!this.on) { this.held = this.pressed = 0; this.steps = []; return; }
    this.#cancel("steps");
    this.#out(panelFrame(this.held, this.pressed, this.steps));
    this.pressed = 0;
    this.steps = [];
    if (this.held) this.#every("panel", PANEL_REPEAT_MS, () => this.#panel());
    else this.#cancel("panel");
  }

  press(id) {
    if (!this.on || id < 0 || id >= (this.hello.buttons || 16) || this.held & (1 << id)) return;
    this.held |= 1 << id;
    this.pressed |= 1 << id;
    this.#panel();
  }

  release(id) {
    if (!(this.held & (1 << id))) return;
    this.held &= ~(1 << id);
    this.#panel();
  }

  flush() {
    this.#panel();
  }

  releaseAll() {
    if (this.held) { this.held = 0; this.#panel(); }
  }

  turn(enc, steps) {
    if (!this.on || enc < 0 || enc >= (this.hello.encoders || 8) || !steps) return;
    while (this.steps.length <= enc) this.steps.push(0);
    const next = this.steps[enc] + steps;
    if (next > 127 || next < -128) { this.#panel(); this.turn(enc, steps); return; }
    this.steps[enc] = next;
    if (this.timers.steps == null) this.timers.steps = this.clock.setTimeout(() => { delete this.timers.steps; this.#panel(); }, STEPS_MS);
  }

  watchPage(win, doc = win.document) {
    const gone = () => { if (this.state !== "off" && this.state !== "stopped" && this.state !== "ended") { this.stop("hidden"); this.hiddenStop = true; } };
    win.addEventListener("pagehide", gone);
    win.addEventListener("beforeunload", gone);
    win.addEventListener("blur", () => this.releaseAll());
    doc?.addEventListener("visibilitychange", () => {
      if (doc.visibilityState === "hidden") gone();
      else if (this.hiddenStop && this.state === "stopped") this.start();
    });
  }
}
