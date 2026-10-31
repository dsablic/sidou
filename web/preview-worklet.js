// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The editor's preview in the audio thread (preview.js makes it): its own copy of SIDOU's wasm
// core plays the song, a pattern or the keys, a block of 128 samples at a time as the FM-1's audio
// interrupt does, through the effect slots, resampled from 44,118 Hz to the context's rate.
import { Core } from "./core.js";
import { Resampler } from "./preview.js";

const TAIL_SECONDS = 2; // after a stop, the releases and the effects' tails ring this long

class SidouPreview extends AudioWorkletProcessor {
  constructor() {
    super();
    this.core = null;
    this.song = null; // the newest song; read at the next start
    this.read = null; // the song the core holds
    this.mode = "idle";
    this.tail = 0;
    this.blocks = 0;
    this.reported = 0;
    this.resampler = null;
    this.port.onmessage = (e) => this.message(e.data);
    this.port.onmessageerror = () => this.say({ type: "error", message: "the preview could not take a message" });
  }

  say(msg) { this.port.postMessage(msg); }

  state(mode) {
    this.mode = mode;
    this.blocks = 0;
    this.reported = 0;
    this.say({ type: "state", state: mode, seconds: 0 });
  }

  load() {
    if (this.read !== this.song) {
      this.core.read(this.song);
      this.read = this.song;
    }
  }

  message(m) {
    try {
      if (m.type === "init") {
        const instance = new WebAssembly.Instance(new WebAssembly.Module(m.bytes), {});
        this.core = new Core(instance.exports);
        this.resampler = new Resampler(this.core.sampleRate, sampleRate, 2);
        return;
      }
      if (m.type === "song") { this.song = m.bytes; return; }
      if (!this.core || !this.song) return;
      if (m.type === "play") {
        this.read = null; // a fresh read: the transport starts from the song as saved
        this.load();
        this.core.play(m.subtune, m.pos);
        this.state("song");
      } else if (m.type === "pattern") {
        this.read = null;
        this.load();
        this.core.playPattern(m.p, m.channel);
        this.state("pattern");
      } else if (m.type === "note") {
        if (this.mode !== "keys") {
          this.read = null;
          this.load();
          this.core.audition();
          this.state("keys");
        }
        this.core.jam(m.voice, m.note, m.instrument);
      } else if (m.type === "release") {
        if (this.mode === "keys") this.core.jam(m.voice, null);
      } else if (m.type === "stop") {
        if (this.mode !== "idle" && this.mode !== "tail") {
          this.core.stop();
          this.mode = "tail";
          this.tail = Math.ceil((TAIL_SECONDS * this.core.sampleRate) / 128);
        }
      }
    } catch (e) {
      this.mode = "idle";
      this.say({ type: "error", message: e.message });
      this.say({ type: "state", state: "idle", seconds: 0 });
    }
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const n = out[0].length;
    if (this.mode === "idle" || !this.core) return true;
    const r = this.resampler;
    while (r.ready < n) {
      const lr = this.core.renderBlocks(1);
      const f = new Float32Array(lr.length);
      for (let i = 0; i < lr.length; i++) f[i] = lr[i] / 32768;
      r.push(f);
      this.blocks++;
      if (this.mode === "tail" && --this.tail <= 0) {
        this.state("idle");
        break;
      }
    }
    const frames = new Float32Array(n * 2);
    const got = r.pull(frames, n);
    for (let i = 0; i < got; i++) {
      out[0][i] = frames[2 * i];
      if (out[1]) out[1][i] = frames[2 * i + 1];
    }
    if (this.mode !== "idle" && this.mode !== "tail" && this.blocks - this.reported >= 86) {
      this.reported = this.blocks;
      this.say({ type: "state", state: this.mode, seconds: (this.blocks * 128) / this.core.sampleRate });
    }
    return true;
  }
}

registerProcessor("sidou-preview", SidouPreview);
