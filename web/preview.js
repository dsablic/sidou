// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The editor's preview: SIDOU's own player and SID (the wasm core, core.js) in an AudioWorklet
// (preview-worklet.js), resampled from the FM-1's 44,118 Hz to the browser's rate. Without
// AudioWorklet (an old browser, a page not served over https or localhost) it renders a stretch
// on the page's thread instead and plays that buffer (the browser resamples it). Nothing sounds
// until the page asks (a press of Play or a key): no autoplay.
import { Core } from "./core.js";

// Linear interpolation from one rate to another, a stream at a time: push() frames in, pull()
// frames out (interleaved, `channels` a frame). At equal rates every frame passes unchanged.
export class Resampler {
  constructor(from, to, channels = 1) {
    this.step = from / to;
    this.channels = channels;
    this.buf = new Float32Array(0);
    this.t = 0; // the next output frame's position in buf, in frames
  }

  push(frames) {
    const keep = Math.floor(this.t);
    const rest = this.buf.subarray(keep * this.channels);
    const next = new Float32Array(rest.length + frames.length);
    next.set(rest);
    next.set(frames, rest.length);
    this.buf = next;
    this.t -= keep;
  }

  // Frames that can come out without more input (one frame after each is needed).
  get ready() {
    const have = this.buf.length / this.channels;
    return Math.max(0, Math.ceil((have - 1 - this.t) / this.step));
  }

  // n frames into out (interleaved); with `flush`, the last frame stands in for the one after.
  pull(out, n, flush = false) {
    const c = this.channels, b = this.buf, have = b.length / c;
    for (let i = 0; i < n; i++) {
      const k = Math.floor(this.t), f = this.t - k;
      const k1 = k + 1 < have ? k + 1 : flush ? have - 1 : -1;
      if (k >= have || k1 < 0) return i;
      for (let ch = 0; ch < c; ch++) out[i * c + ch] = f ? b[k * c + ch] * (1 - f) + b[k1 * c + ch] * f : b[k * c + ch];
      this.t += this.step;
    }
    return n;
  }
}

// A whole buffer at once (mono): for the tests and the fallback's checks.
export function resample(samples, from, to) {
  const r = new Resampler(from, to, 1);
  r.push(samples);
  const out = new Float32Array(Math.round((samples.length * to) / from));
  return out.subarray(0, r.pull(out, out.length, true));
}

// The page's side. `wasm` is sidou-core.wasm's bytes. onMessage gets the worklet's news:
// { type: "state", state: "idle" | "song" | "pattern" | "keys", seconds }, { type: "error", message }.
export class Preview {
  constructor(wasm, { onMessage = () => {} } = {}) {
    this.wasm = wasm;
    this.onMessage = onMessage;
    this.ctx = null;
    this.node = null;
    this.song = null;
    this.fallback = null;
    this.source = null;
  }

  get worklet() { return !!this.node; }

  // The audio context and the worklet, made on the first press (browsers start audio only then).
  async ensure() {
    if (this.ctx) {
      if (this.ctx.state === "suspended") await this.ctx.resume();
      return;
    }
    const Ctx = globalThis.AudioContext ?? globalThis.webkitAudioContext;
    if (!Ctx) throw new Error("this browser has no Web Audio");
    this.ctx = new Ctx({ latencyHint: "interactive" });
    if (this.ctx.audioWorklet && globalThis.AudioWorkletNode) {
      try {
        await this.ctx.audioWorklet.addModule(new URL("preview-worklet.js", import.meta.url));
        this.node = new AudioWorkletNode(this.ctx, "sidou-preview", { numberOfInputs: 0, outputChannelCount: [2] });
        this.node.port.onmessage = (e) => this.onMessage(e.data);
        this.node.onprocessorerror = () => this.onMessage({ type: "error", message: "the preview stopped with an error" });
        // the module's bytes (a compiled WebAssembly.Module does not reach an AudioWorklet in
        // every browser): the worklet compiles them itself
        this.node.port.postMessage({ type: "init", bytes: this.wasm });
        this.node.connect(this.ctx.destination);
      } catch (e) {
        this.node = null;
        console.warn(`preview: no AudioWorklet (${e.message}); rendering ahead instead`);
      }
    }
    if (!this.node) {
      const { instance } = await WebAssembly.instantiate(this.wasm, {});
      this.fallback = new Core(instance.exports);
    }
    if (this.song) this.setSong(this.song);
    if (this.ctx.state === "suspended") await this.ctx.resume();
  }

  // The song as it is now (a .sng): what the next play, pattern or key uses.
  setSong(bytes) {
    this.song = bytes;
    this.node?.port.postMessage({ type: "song", bytes });
  }

  async #send(msg, render) {
    await this.ensure();
    if (this.node) return this.node.port.postMessage(msg);
    this.#stopSource();
    try {
      this.fallback.read(this.song);
      const lr = render(this.fallback);
      this.#playBuffer(lr);
      this.onMessage({ type: "state", state: msg.type === "play" ? "song" : msg.type, seconds: lr.length / 2 / 44118 });
    } catch (e) {
      this.onMessage({ type: "error", message: e.message });
    }
  }

  #playBuffer(lr) {
    const n = lr.length / 2;
    const buf = this.ctx.createBuffer(2, n, this.fallback.sampleRate);
    const l = buf.getChannelData(0), r = buf.getChannelData(1);
    for (let i = 0; i < n; i++) { l[i] = lr[2 * i] / 32768; r[i] = lr[2 * i + 1] / 32768; }
    this.source = this.ctx.createBufferSource();
    this.source.buffer = buf;
    this.source.connect(this.ctx.destination);
    this.source.onended = () => this.onMessage({ type: "state", state: "idle" });
    this.source.start();
  }

  #stopSource() {
    if (!this.source) return;
    this.source.onended = null;
    try { this.source.stop(); } catch { /* already ended */ }
    this.source = null;
  }

  // The song from orderlist position `pos` (the fallback renders 30 seconds of it).
  play(subtune = 0, pos = 0) {
    return this.#send({ type: "play", subtune, pos }, (c) => { c.play(subtune, pos); return c.renderOut(30); });
  }

  // Pattern p over and over on voice `channel` (the fallback: 8 seconds).
  pattern(p, channel = 0) {
    return this.#send({ type: "pattern", p, channel }, (c) => { c.playPattern(p, channel); return c.renderOut(8); });
  }

  // A key down: instrument i at note (0 C-0 .. 95) on voice 0-2 until release (the fallback: half
  // a second held, a second to ring).
  note(voice, note, instrument) {
    return this.#send({ type: "note", voice, note, instrument }, (c) => {
      c.audition();
      c.jam(0, note, instrument);
      const held = c.renderOut(0.5);
      c.jam(0, null);
      const rest = c.renderOut(1);
      const out = new Int16Array(held.length + rest.length);
      out.set(held);
      out.set(rest, held.length);
      return out;
    });
  }

  release(voice) { this.node?.port.postMessage({ type: "release", voice }); }

  stop() {
    this.node?.port.postMessage({ type: "stop" });
    this.#stopSource();
    if (!this.node) this.onMessage({ type: "state", state: "idle" });
  }

  close() {
    this.stop();
    this.ctx?.close();
    this.ctx = null;
    this.node = null;
  }
}
