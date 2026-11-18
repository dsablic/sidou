// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCore } from "./core.js";
import { fromLink, loadPatcher, shareableLink } from "./patch.js";
import { convert } from "./sidconv.js";
import { soundsOf, textOf, tuneSounds } from "./sidsound.js";
import { loadTune } from "./tune.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const tune = await loadTune(readFileSync(process.argv[2] ?? `${root}build/wasm-tune/sidou-tune.wasm`));
const coreBytes = readFileSync(process.argv[3] ?? `${root}build/wasm-core/sidou-core.wasm`);
const core = await loadCore(coreBytes);
const patcher = await loadPatcher(readFileSync(process.argv[4] ?? `${root}build/wasm-patch/sidou-patch.wasm`), coreBytes);
const STARTER = ["sidou-loop", "chrome-comet", "iron-rite", "neon-highway", "paper-lanterns", "consultant", "dojo", "sanction", "transylvanian", "unleash"];
const SECONDS = 30, LAB_TABLE_STEPS = 16, AUDIBLE = 512, PWM_STEP_PERCENT = 1, CUTOFF_AT = 5;
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const converted = new Map();
const songOf = (name) => {
  if (!converted.has(name)) {
    tune.read(readFileSync(`${root}assets/starter/${name}.sid`));
    converted.set(name, convert(tune.capture({ song: 0, seconds: SECONDS }), { freqTable: core.freqTable() }).song);
  }
  return converted.get(name);
};

const tables = ({ wave = [], pulse = [], filter = [] }) => ({
  left: [wave.map((r) => r[0]), pulse.map((r) => r[0]), filter.map((r) => r[0]), []],
  right: [wave.map((r) => r[1]), pulse.map((r) => r[1]), filter.map((r) => r[1]), []],
});
const instrument = (over = {}) => ({ ad: 0x29, sr: 0xa6, wave: 1, pulse: 0, filter: 0, first: 0x09, voice: "A1", uses: 3, ...over });
const soundFor = (ins, t) => soundsOf({ instruments: [ins], tables: tables(t) }, { cutoffHz: (fc) => fc * 10 })[0];

const GATE = 0x01, TRI = 0x10, SAW = 0x20, PULSE = 0x40, NOISE = 0x80, JUMP = 0xff, KEEP = 0x80;

await test("an instrument's envelope, waveform and pulse sweep become LAB's envelope, wave and PWM", () => {
  const s = soundFor(instrument({ pulse: 1 }), {
    wave: [[PULSE | GATE, 0], [JUMP, 0]],
    pulse: [[0x88, 0x00], [40, 0x10], [40, 0xf0], [JUMP, 2]],
  });
  assert.deepEqual(s.patch.envelope, { attack: 2, decay: 9, sustain: 10, release: 6 });
  assert.equal(s.patch.wave, "pulse");
  assert.equal(s.patch.pulse_width, 57.81);
  assert.deepEqual(s.patch.pwm, { depth: 7.81, rate_hz: 0.63 });
  assert.equal(s.patch.table, undefined);
  assert.ok(s.simplified.some((t) => /PWM/.test(t)));
  assert.match(s.about, /pulse sweep/);
});

await test("a wave table becomes LAB's table: waves, relative, falling, held and fixed notes, and its loop", () => {
  const s = soundFor(instrument(), {
    wave: [[NOISE | GATE, 0xa4], [PULSE | GATE, 0x00], [0, 0x07], [SAW | GATE, 0x7e], [TRI | GATE | 0x04, KEEP], [JUMP, 3]],
  });
  assert.deepEqual(s.patch.table.steps, [
    { wave: "noise", note: "C3" },
    { wave: "pulse", note: 0 },
    { wave: "pulse", note: 7 },
    { wave: "saw", note: -2 },
    { wave: "triangle", ring: true, note: "hold" },
  ]);
  assert.equal(s.patch.table.loop, 3);
  assert.equal(s.patch.wave, "triangle");
  assert.match(s.about, /noise then pulse then saw then triangle, arpeggio/);
});

await test("steps that repeat at the end are one, and a plain instrument has no table", () => {
  assert.equal(soundFor(instrument(), { wave: [[SAW | GATE, 0], [0, 0], [0, 0], [JUMP, 0]] }).patch.table, undefined);
  const s = soundFor(instrument(), { wave: [[NOISE | GATE, 0], [PULSE | GATE, 0], [0, 0], [0, 0], [JUMP, 0]] });
  assert.equal(s.patch.table.steps.length, 2);
});

await test("a table longer than LAB's is cut, its loop kept when inside the cut and dropped when past it, and it says so", () => {
  const rows = Array.from({ length: 20 }, (_, k) => [(k % 2 ? SAW : PULSE) | GATE, k]);
  const looped = soundFor(instrument(), { wave: [...rows, [JUMP, 18]] });
  assert.equal(looped.patch.table.steps.length, LAB_TABLE_STEPS);
  assert.equal(looped.patch.table.loop, 0);
  assert.ok(looped.simplified.some((t) => /cut to LAB's 16 steps/.test(t)));
  assert.equal(soundFor(instrument(), { wave: [...rows, [JUMP, 3]] }).patch.table.loop, 3);
  const early = soundFor(instrument(), { wave: [...rows.slice(0, 5), [JUMP, 2]] });
  assert.equal(early.patch.table.loop, 2);
  assert.equal(early.simplified.length, 0);
});

await test("a filter keeps its mode, cutoff and resonance on every voice, and says it dropped the tune's moves and its own-voice routing", () => {
  const withRoute = (route) => soundFor(instrument({ filter: 1 }), { wave: [[SAW | GATE, 0], [JUMP, 0]], filter: [[0x90, 0x70 | route], [0x00, 0x40], [JUMP, 0]] });
  const own = withRoute(0x01);
  assert.deepEqual(own.patch.filter, { mode: "low", cutoff_hz: 5120, resonance: 7, voices: [1, 2, 3] });
  assert.ok(own.simplified.some((t) => /tune's moves/.test(t)));
  assert.ok(own.simplified.some((t) => /LAB filters every voice/.test(t)));
  assert.ok(!withRoute(0x07).simplified.some((t) => /LAB filters every voice/.test(t)));
  const none = soundFor(instrument({ filter: 1 }), { wave: [[SAW | GATE, 0], [JUMP, 0]], filter: [[0x80, 0x00], [0x00, 0x40], [JUMP, 0]] });
  assert.deepEqual(none.patch.filter, { voices: [] });
});

await test("LAB's cutoff in Hz is the one the converter reads back as the register value, the default's too", () => {
  for (const fc of [0, 8, 800, 1199, 1200, 1201, 2047]) {
    const patch = { format: "sidou-patch/1", name: "CUT", filter: { cutoff_hz: patcher.cutoffHz(fc), voices: [1, 2, 3] } };
    core.read(patcher.check(JSON.stringify(patch)).sng);
    assert.equal(new DataView(core.kept(1).sound.buffer, core.kept(1).sound.byteOffset).getUint16(CUTOFF_AT, true), fc, `fc ${fc}`);
  }
});

await test("every instrument of ten converted starter tunes is a valid, audible sound that LAB holds as the mapping says", () => {
  let count = 0;
  for (const name of STARTER) {
    const song = songOf(name);
    const sounds = tuneSounds(song, patcher);
    assert.equal(sounds.length, song.instruments.length, name);
    sounds.forEach((s, k) => {
      const ins = song.instruments[k];
      assert.equal(s.refused, "", `${name} ${s.name}: ${s.refused}`);
      const checked = patcher.check(s.text);
      assert.equal(checked.ok, true, s.name);
      core.read(checked.sng);
      const back = JSON.parse(patcher.write(core.kept(1).sound, core.kept(1).name));
      const given = JSON.parse(s.text);
      assert.deepEqual(back.envelope ?? {}, { attack: ins.ad >> 4, decay: ins.ad & 15, sustain: ins.sr >> 4, release: ins.sr & 15 }, `${name} ${s.name} envelope`);
      assert.equal(back.wave, given.wave, `${name} ${s.name} wave`);
      assert.deepEqual(back.table?.steps?.map((t) => t.wave ?? back.wave), given.table?.steps.map((t) => t.wave), `${name} ${s.name} table`);
      assert.equal(back.table?.loop ?? 0, given.table?.loop ?? 0);
      assert.equal(!!back.pwm, !!given.pwm, `${name} ${s.name} pwm`);
      if (given.pwm) assert.ok(Math.abs(back.pwm.depth - given.pwm.depth) <= PWM_STEP_PERCENT, `${name} ${s.name} pwm depth`);
      assert.ok(core.renderInstrument(1).some((v) => Math.abs(v) > AUDIBLE), `${name} ${s.name} is heard`);
      count++;
    });
  }
  assert.ok(count >= 100, `${count} sounds`);
});

await test("a tune's sounds say which voice they are on and how many notes use them, and each is a link a page opens", () => {
  const page = "https://dsablic.github.io/sidou/library.html";
  const song = songOf("dojo");
  const sounds = tuneSounds(song, patcher);
  assert.match(sounds[0].about, /^A1, \d+ notes?: /);
  assert.ok(song.instruments.every((i) => i.uses > 0 && /^[AB][1-3]$/.test(i.voice)));
  for (const s of sounds) {
    const made = shareableLink(page, s.text);
    assert.equal(made.error, undefined, s.name);
    assert.equal(JSON.stringify(JSON.parse(fromLink(new URL(made.link).hash).text)), JSON.stringify(JSON.parse(s.text)));
  }
  assert.equal(textOf(JSON.parse(sounds[0].text)), sounds[0].text);
});

console.log(`sidsound: ${n} tests passed`);
