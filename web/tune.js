// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
export class TuneError extends Error {
  constructor(message) {
    super(message);
    this.name = "TuneError";
  }
}

const PSID_ERR = ["OK", "not a .sid file", "an RSID tune (it needs a whole C64: not yet)", "a broken header",
  "a Sidplayer .mus tune", "too long to load", "the load address does not fit"];
const RUN = { INIT_DONE: 1, CALL_DONE: 2, IDLE: 3, INIT_TOO_LONG: 4, PLAY_HUNG: 5, JAM: 6 };
const STOPPED = { [RUN.INIT_TOO_LONG]: "its init ran too long", [RUN.PLAY_HUNG]: "its play routine hung",
  [RUN.JAM]: "its code jammed the 6502" };
const INFO_WORDS = 15, STRING = 64, WRITE_BYTES = 8;
const MODELS = ["unknown", "6581", "8580", "6581 and 8580"];
const CLOCKS = ["unknown", "PAL", "NTSC", "PAL and NTSC"];

const text = (b) => {
  const end = b.indexOf(0);
  return new TextDecoder("latin1").decode(end < 0 ? b : b.subarray(0, end));
};

export async function loadTune(source) {
  const bytes = source instanceof ArrayBuffer || ArrayBuffer.isView(source) ? source : await (await fetch(source)).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Tune(instance.exports);
}

export class Tune {
  constructor(x) {
    this.x = x;
    this.header = null;
  }

  get mem() { return this.x.memory; }

  read(bytes) {
    if (bytes.length > this.x.tune_in_max()) throw new TuneError(".sid: too big");
    new Uint8Array(this.mem.buffer, this.x.tune_in(), bytes.length).set(bytes);
    const r = this.x.tune_load(bytes.length);
    if (r !== 0) throw new TuneError(`.sid: ${PSID_ERR[r] ?? `refused (${r})`}`);
    const p = this.x.tune_info();
    const w = new Uint32Array(this.mem.buffer, p, INFO_WORDS);
    const s = (k) => text(new Uint8Array(this.mem.buffer, p + INFO_WORDS * 4 + k * STRING, STRING));
    const [, version, load, init, play, songs, start, flags, speed, sid2, sid3, clock, model1, model2, sids] = w;
    this.header = { version, load, init, play, songs, start, flags, speed, sid2, sid3, clock, model1, model2, sids,
      chips: sid2 ? 2 : 1, ntsc: clock === 2, model: MODELS[model1] ?? MODELS[0], clockName: CLOCKS[clock] ?? CLOCKS[0],
      name: s(0), author: s(1), released: s(2) };
    return this.header;
  }

  #writes() {
    const n = this.x.tune_write_count();
    const b = new DataView(this.mem.buffer, this.x.tune_writes(), n * WRITE_BYTES);
    const start = this.x.tune_call_start();
    const out = new Array(n);
    for (let k = 0; k < n; k++) {
      const o = k * WRITE_BYTES;
      out[k] = [b.getUint32(o, true) - start, b.getUint8(o + 4), b.getUint8(o + 5), b.getUint8(o + 6)];
    }
    return out;
  }

  capture({ song = 0, seconds = 60 } = {}) {
    if (!this.header) throw new TuneError("no tune loaded");
    const pick = song || this.header.start;
    let code = this.x.tune_start(pick);
    if (code < 0) throw new TuneError(`.sid: there is no song ${pick} (it has ${this.header.songs})`);
    const clock = this.x.tune_clock(), total = seconds * clock;
    const calls = [];
    let dropped = 0, stopped = "";
    for (;;) {
      calls.push({ start: this.x.tune_call_start(), writes: this.#writes() });
      dropped += this.x.tune_dropped();
      if (code !== RUN.INIT_DONE && code !== RUN.CALL_DONE) {
        stopped = STOPPED[code] ?? `it stopped (${code})`;
        break;
      }
      if (((this.x.tune_next_due() - total) | 0) >= 0) break;
      code = this.x.tune_call();
    }
    return { song: pick, seconds, clock, chips: this.header.chips, ntsc: this.header.ntsc, calls, dropped, stopped };
  }
}
