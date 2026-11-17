// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadCore } from "./core.js";
import {
  ConvertError, GT2_MAX_INSTRUMENTS, MAX_INSTRUMENTS_A_VOICE, closeness, convert, framesOfCapture, framesOfPlayer,
  makePitch, noteLists, previewSong, tracksOf, frameMatch,
} from "./sidconv.js";
import { loadTune } from "./tune.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
const tune = await loadTune(readFileSync(process.argv[2] ?? `${root}build/wasm-tune/sidou-tune.wasm`));
const core = await loadCore(readFileSync(process.argv[3] ?? `${root}build/wasm-core/sidou-core.wasm`));
const freqTable = core.freqTable();
const SECONDS = 30, FRAMES = SECONDS * 50, EV_ROW = 1, EV_NOTE = 2;
let n = 0;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const STARTER = [
  { tune: "sidou-loop", sng: "assets/songs/sidou-loop.sng", melodic: [0, 1] },
  { tune: "chrome-comet", sng: "assets/songs/chrome-comet.sng", melodic: [0, 1] },
  { tune: "iron-rite", sng: "assets/songs/iron-rite.sng", melodic: [0, 1] },
  { tune: "neon-highway", sng: "assets/songs/neon-highway.sng", melodic: [0, 1] },
  { tune: "paper-lanterns", sng: "assets/songs/paper-lanterns.sng", melodic: [1, 2] },
  { tune: "consultant", sng: "third_party/goattracker2/examples/consultant.sng", melodic: [0, 2] },
  { tune: "dojo", sng: "third_party/goattracker2/examples/dojo.sng", melodic: [0, 1, 2] },
  { tune: "sanction", sng: "third_party/goattracker2/examples/sanction.sng", melodic: [0, 1, 2] },
  { tune: "transylvanian", sng: "third_party/goattracker2/examples/transylvanian.sng", melodic: [0, 2] },
  { tune: "unleash", sng: "third_party/goattracker2/examples/unleash.sng", melodic: [0, 1] },
];

const captureOf = (bytes, seconds = SECONDS, song = 0) => {
  tune.read(bytes);
  return tune.capture({ song, seconds });
};
const converted = new Map();
const conversionOf = (name) => {
  if (!converted.has(name)) {
    const run = captureOf(readFileSync(`${root}assets/starter/${name}.sid`));
    converted.set(name, { run, ...convert(run, { freqTable }) });
  }
  return converted.get(name);
};

const sourceOf = (sng) => {
  core.read(readFileSync(`${root}${sng}`));
  const played = core.frames(FRAMES);
  const rowGaps = [];
  let last = null;
  played.frames.forEach((f, i) => {
    if (f.events[0].flags & EV_ROW) { if (last !== null) rowGaps.push(i - last); last = i; }
  });
  const notes = [0, 1, 2].map((ch) => played.frames.flatMap((f, i) => (f.events[ch].flags & EV_NOTE ? [{ frame: i, note: f.events[ch].note }] : [])));
  return { firstRows: rowGaps.slice(0, 2), notes };
};

const rowsOfSong = (song) => (song.swing ? song.swing : [song.tempo, song.tempo]);

for (const s of STARTER) {
  await test(`${s.tune}: the row length is the source song's`, () => {
    const { song } = conversionOf(s.tune);
    const { firstRows } = sourceOf(s.sng);
    assert.deepEqual([...rowsOfSong(song)].sort(), [...firstRows].sort());
  });
  await test(`${s.tune}: the melodic voices' notes are the source song's`, () => {
    const { run } = conversionOf(s.tune);
    const inferred = noteLists(framesOfCapture(run), makePitch(freqTable, run.clock));
    const truth = sourceOf(s.sng).notes;
    const c = closeness(truth, inferred);
    for (const v of s.melodic) assert.ok(c[v].score >= 0.95, `voice ${v + 1}: ${c[v].matched} of ${c[v].original} notes`);
  });
}

await test("instruments stay bounded: a handful a voice, within GoatTracker's 63", () => {
  for (const s of STARTER) {
    const { song } = conversionOf(s.tune);
    assert.ok(song.instruments.length <= Math.min(GT2_MAX_INSTRUMENTS, MAX_INSTRUMENTS_A_VOICE * song.channels), `${s.tune}: ${song.instruments.length}`);
  }
});

const playedNotes = (bytes, voices, pitch) => {
  core.read(bytes);
  return noteLists(framesOfPlayer(core.frames(FRAMES)), pitch, voices);
};

for (const s of STARTER) {
  await test(`${s.tune}: the .sng loads in the FM-1's reader and its player plays the original's notes on time`, () => {
    const { run, bytes, song } = conversionOf(s.tune);
    const info = core.read(bytes);
    assert.equal(info.channels, song.channels);
    const pitch = makePitch(freqTable, run.clock);
    const original = noteLists(framesOfCapture(run), pitch);
    const c = closeness(original, playedNotes(bytes, original.length, pitch));
    c.forEach((v, k) => {
      assert.ok(v.score >= 0.97, `voice ${k + 1}: ${v.matched} of ${v.original} notes`);
      assert.ok(v.timing <= 0.1, `voice ${k + 1}: ${v.timing} frames off on average`);
    });
  });
}

await test("the conversion keeps most frames' waveform and pitch", () => {
  for (const name of ["sidou-loop", "dojo", "neon-highway"]) {
    const { run, bytes } = conversionOf(name);
    const pitch = makePitch(freqTable, run.clock);
    const original = noteLists(framesOfCapture(run), pitch);
    core.read(bytes);
    const played = framesOfPlayer(core.frames(FRAMES));
    const { lag } = closeness(original, noteLists(played, pitch, original.length))[0];
    const share = frameMatch(tracksOf(framesOfCapture(run)[0]).voices, tracksOf(played[0]).voices, lag, pitch);
    share.forEach((x, v) => assert.ok(x >= 0.9, `${name} voice ${v + 1}: ${(x * 100).toFixed(0)} %`));
  }
});

await test("swing comes out as funktempo, tempo changes as SET TEMPO on their rows", () => {
  const lanterns = conversionOf("paper-lanterns").song;
  assert.deepEqual(lanterns.swing, [9, 7]);
  const funk = lanterns.patterns.flat().filter((c) => c[2] === 0xe);
  assert.equal(funk.length, 1);
  assert.deepEqual([lanterns.tables.left[3][funk[0][3] - 1], lanterns.tables.right[3][funk[0][3] - 1]], [9, 7]);
  const whip = conversionOf("transylvanian").song;
  const tempos = whip.patterns.flat().filter((c) => c[2] === 0xf).map((c) => c[3]);
  assert.deepEqual([...new Set(tempos)].sort(), [6, 7]);
});

const asm = (origin, program) => {
  const labels = {}, out = [];
  const size = (x) => (typeof x === "number" ? 1 : x.word ? 2 : x.rel ? 1 : 0);
  let at = origin;
  for (const x of program) { if (x.label) labels[x.label] = at; at += size(x); }
  at = origin;
  for (const x of program) {
    if (typeof x === "number") out.push(x);
    else if (x.word) { const v = typeof x.word === "string" ? labels[x.word] : x.word; out.push(v & 0xff, v >> 8); }
    else if (x.rel) out.push((labels[x.rel] - (at + 1)) & 0xff);
    at += size(x);
  }
  return out;
};

const LDA_IMM = 0xa9, STA_ABS = 0x8d, STA_ZP = 0x85, INC_ZP = 0xe6, LDA_ZP = 0xa5, AND_IMM = 0x29, BNE = 0xd0,
  LDX_ZP = 0xa6, LDA_ABS_X = 0xbd, INX = 0xe8, TXA = 0x8a, RTS = 0x60, JMP = 0x4c;
const store = (value, ...addresses) => [LDA_IMM, value, ...addresses.flatMap((a) => [STA_ABS, { word: a }])];

const synthetic = ({ notesA, notesB, digi = false }) => {
  const note = (list, k) => freqTable[list[k]];
  const table = (label, list, part) => [{ label }, ...list.map((_, k) => (part ? note(list, k) >> 8 : note(list, k) & 0xff))];
  const gate = (reg, wave) => [...store(wave, reg), ...store(wave | 1, reg)];
  const code = asm(0x1000, [
    JMP, { word: "init" }, JMP, { word: "play" },
    { label: "init" }, ...store(0x0f, 0xd418, 0xd438), ...store(0x09, 0xd405, 0xd425), ...store(0xf0, 0xd406, 0xd426),
    LDA_IMM, 0, STA_ZP, 0xfb, STA_ZP, 0xfc, RTS,
    { label: "play" }, ...(digi ? [...store(0x0f, 0xd418), ...store(0x08, 0xd418), ...store(0x0f, 0xd418), ...store(0x04, 0xd418)] : []),
    INC_ZP, 0xfb, LDA_ZP, 0xfb, AND_IMM, 0x0f, BNE, { rel: "out" },
    LDX_ZP, 0xfc,
    LDA_ABS_X, { word: "aLo" }, STA_ABS, { word: 0xd400 }, LDA_ABS_X, { word: "aHi" }, STA_ABS, { word: 0xd401 },
    LDA_ABS_X, { word: "bLo" }, STA_ABS, { word: 0xd420 }, LDA_ABS_X, { word: "bHi" }, STA_ABS, { word: 0xd421 },
    ...gate(0xd404, 0x20), ...gate(0xd424, 0x10),
    INX, TXA, AND_IMM, notesA.length - 1, STA_ZP, 0xfc,
    { label: "out" }, RTS,
    ...table("aLo", notesA, 0), ...table("aHi", notesA, 1), ...table("bLo", notesB, 0), ...table("bHi", notesB, 1),
  ]);
  const h = new Uint8Array(0x7c);
  h.set([0x50, 0x53, 0x49, 0x44, 0, 3, 0, 0x7c, 0x10, 0x00, 0x10, 0x00, 0x10, 0x03, 0, 1, 0, 1]);
  h.set(new TextEncoder().encode("TWO CHIPS"), 0x16);
  h[0x77] = 0x14;
  h[0x7a] = 0x42;
  const file = new Uint8Array(h.length + code.length);
  file.set(h);
  file.set(code, h.length);
  return file;
};

const SCALE = [48, 50, 52, 53, 55, 57, 59, 60], FIFTHS = SCALE.map((x) => x + 7);

await test("a 2SID tune becomes a six-voice GoatTracker Stereo song, both chips' notes kept", () => {
  const run = captureOf(synthetic({ notesA: SCALE, notesB: FIFTHS }), 10);
  const c = convert(run, { freqTable });
  assert.equal(c.song.channels, 6);
  assert.equal(core.read(c.bytes).channels, 6);
  const notes = c.analysis.voices.map((v) => v.notes.map((x) => x.note));
  assert.deepEqual(notes[0].slice(0, 8), SCALE);
  assert.deepEqual(notes[3].slice(0, 8), FIFTHS);
  assert.ok(c.notes.some((t) => /six voices/.test(t)));
  const chipA = previewSong(c.song);
  assert.equal(core.read(chipA).channels, 3);
  const pitch = makePitch(freqTable, run.clock);
  const played = noteLists(framesOfPlayer(core.frames(500)), pitch, 3);
  assert.deepEqual(played[0].slice(0, 8).map((x) => x.note), SCALE);
});

await test("fast volume writes are named as dropped digi samples, an ordinary tune is not", () => {
  const digi = convert(captureOf(synthetic({ notesA: SCALE, notesB: SCALE, digi: true }), 4), { freqTable });
  assert.ok(digi.notes.some((t) => /digi/.test(t)));
  assert.ok(!conversionOf("dojo").notes.some((t) => /digi/.test(t)));
});

await test("a silent tune is refused in words", () => {
  const silent = synthetic({ notesA: SCALE, notesB: SCALE });
  const patched = silent.slice();
  patched.set([JMP, 0x06, 0x10], 0x7c + 3);
  assert.throws(() => convert(captureOf(patched, 2), { freqTable }), (e) => e instanceof ConvertError && /no notes/.test(e.message));
});

await test("the simplifications are listed in words", () => {
  const { notes } = conversionOf("transylvanian");
  assert.ok(notes.some((t) => /hard restart/.test(t)));
  assert.ok(notes.some((t) => /tempo change/.test(t)));
});

console.log(`sidconv: ${n} tests passed`);
