// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { Core } from "./core.js";

export class Resampler {
  constructor(from, to, channels = 1) {
    this.step = from / to;
    this.channels = channels;
    this.buf = new Float32Array(0);
    this.t = 0;
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

  get ready() {
    const have = this.buf.length / this.channels;
    return Math.max(0, Math.ceil((have - 1 - this.t) / this.step));
  }

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

export function resample(samples, from, to) {
  const r = new Resampler(from, to, 1);
  r.push(samples);
  const out = new Float32Array(Math.round((samples.length * to) / from));
  return out.subarray(0, r.pull(out, out.length, true));
}

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
    try { this.source.stop(); } catch {}
    this.source = null;
  }

  play(subtune = 0, pos = 0) {
    return this.#send({ type: "play", subtune, pos }, (c) => { c.play(subtune, pos); return c.renderOut(30); });
  }

  pattern(p, channel = 0) {
    return this.#send({ type: "pattern", p, channel }, (c) => { c.playPattern(p, channel); return c.renderOut(8); });
  }

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
