// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { CLEAR, FIRST_FRAME, FRAMES, SLOTS } from "./sidart.js";

export const SIDE = 240;
const SKY = SLOTS.find((s) => s.sky);
const BASS_SPLIT = 50;
const BASS = { none: 0, low: 1, high: 2 };
const HITS = { k: 1, s: 2, h: 4, a: 8 };
const REFUSALS = [
  null,
  "there is no pack",
  "too short, or over the store's 65,536 bytes a file",
  "not a SIDOU art pack (.sidart)",
  "a newer art pack format than this SIDOU reads",
  "art for another scene than RED MOON RITE",
  "its pack name is not 1 to 16 characters of the screen font",
  "its slots are not RED MOON RITE's: every image at its size and frame count",
  "a frame is outside its slot or patches another size",
  "a row table or a row is outside the pack or its page",
  "its CRC does not match its bytes",
];

export const refusalWords = (code) => (code === 0 ? null : REFUSALS[code] ?? `refused (${code})`);

export function parseTrace(text) {
  return text.split("\n").filter((l) => l && !l.startsWith("#")).map((l) => {
    const [, steps, cutoff] = l.split(" ");
    return {
      cutoff: Number(cutoff),
      beats: steps === "-" ? [] : steps.split(",").map((step) => {
        const [s, bar, flags, bass] = step.split(".");
        return {
          s: Number(s), bar: Number(bar),
          hits: [...flags].reduce((h, c) => h | (HITS[c] ?? 0), 0),
          bass: bass === "-" ? BASS.none : Number(bass) >= BASS_SPLIT ? BASS.high : BASS.low,
        };
      }),
    };
  });
}

export async function loadArtCore(wasm) {
  const { instance } = await WebAssembly.instantiate(wasm, {});
  return new ArtCore(instance.exports);
}

class ArtCore {
  constructor(x) {
    this.x = x;
    this.ok = false;
  }
  bytes(at, n) { return new Uint8Array(this.x.memory.buffer, at, n); }
  check(pack) {
    this.bytes(this.x.art_pack_buffer(), pack.length).set(pack);
    const code = this.x.art_pack_check(pack.length);
    this.ok = code === 0;
    return code;
  }
  frames() {
    return Array.from({ length: FRAMES }, (_, i) => {
      const s = SLOTS[FIRST_FRAME.findLastIndex((f) => f <= i)];
      const px = new Uint8Array(s.w * s.h);
      for (let y = 0; y < s.h; y++) px.set(this.bytes(this.x.art_frame_row(i, y), s.w), y * s.w);
      return px;
    });
  }
  skies() {
    return SKY.frames.map((_, k) => {
      const px = new Uint8Array(SKY.w * SKY.h);
      for (let y = 0; y < SKY.h; y++) px.set(this.bytes(this.x.art_sky_row(k, y), SKY.w), y * SKY.w);
      return px;
    });
  }
  start() { this.x.art_play_start(); }
  play({ beats, cutoff }) {
    for (const b of beats) this.x.art_play_beat(b.s, b.bar, b.hits, b.bass);
    return this.bytes(this.x.art_play_frame(cutoff), SIDE * SIDE).slice();
  }
}

export const overSky = (overlay, sky) => overlay.map((p, k) => (p === CLEAR ? sky[k] : p));
