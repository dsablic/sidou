// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { FIRSTNOTE, MIDI_GT2_C0, SongError, callRate, parseSong, songToMidi, toMidi, walkSong } from "./songmidi.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = `${here}../`;
let n = 0;
const SONGS_WITH_GT2_EXAMPLES = 17, WRONG_PITCH_AT_MOST_ONE_IN = 2000;
const test = async (name, fn) => { await fn(); n++; console.log(`ok - ${name}`); };

const songFiles = [];
for (const dir of ["assets/songs", "third_party/goattracker2/examples"]) {
  if (!existsSync(`${root}${dir}`)) continue;
  for (const f of readdirSync(`${root}${dir}`).filter((f) => f.endsWith(".sng")).sort()) {
    const stem = f.replace(/\.sng$/, "");
    if (existsSync(`${here}songmidi-fixtures/${stem}.json`)) songFiles.push({ stem, path: `${root}${dir}/${f}` });
  }
}
const showcase = ["chrome-comet", "iron-rite", "neon-highway", "paper-lanterns"];
assert.ok(showcase.every((s) => songFiles.some((f) => f.stem === s)), "the showcase songs and their fixtures");
const fixture = (stem) => JSON.parse(readFileSync(`${here}songmidi-fixtures/${stem}.json`, "utf8"));

function gateRuns(notes) {
  const runs = [];
  for (const nt of notes) {
    const last = runs[runs.length - 1];
    if (nt.tie && last && last.end === nt.start) {
      last.end = nt.end;
      last.notes.push(nt);
    } else {
      runs.push({ start: nt.start, end: nt.end, notes: [nt] });
    }
  }
  return runs;
}

let compared = 0, ties = 0, truncated = 0, arps = 0, calls = 0, wrong = 0, glided = 0;
function comparePitches(stem, v, notes, course, end) {
  const at = (c) => {
    let lo = 0, hi = course.length - 1;
    while (lo < hi) {
      const m = (lo + hi + 1) >> 1;
      if (course[m][0] <= c) lo = m; else hi = m - 1;
    }
    return course[lo][1];
  };
  for (const nt of notes) {
    for (let c = nt.start + 1; c < Math.min(nt.end, end); c++) {
      const ref = at(c);
      if (ref < 0) { glided++; continue; }
      calls++;
      if (ref !== nt.pitch - MIDI_GT2_C0) wrong++;
    }
  }
}

for (const { stem, path } of songFiles) {
  await test(`${stem}: every note's start, end and pitch match GoatTracker's player`, () => {
    const bytes = readFileSync(path);
    const song = parseSong(bytes);
    const walk = walkSong(song, 0);
    const fx = fixture(stem);
    assert.ok(fx.calls >= Math.max(...walk.ends) + 20, `${stem}: the fixture has ${fx.calls} calls, the walk ends at ${Math.max(...walk.ends)}`);
    assert.equal(song.channels, 3);
    for (let v = 0; v < 3; v++) {
      const end = walk.ends[v];
      const mine = gateRuns(walk.notes.filter((x) => x.voice === v));
      const theirs = fx.voices[v].filter(([s]) => s < end).map(([s, e, f]) => [s, Math.min(e, end), f, e > end]);
      assert.deepEqual(mine.map((r) => [r.start, r.end]), theirs.map(([s, e]) => [s, e]), `${stem} voice ${v + 1}: gate runs`);
      compared += mine.length;
      for (const [i, r] of mine.entries()) {
        if (theirs[i][3]) truncated++;
        ties += r.notes.length - 1;
      }
      comparePitches(stem, v, walk.notes.filter((x) => x.voice === v), fx.course[v], end);
      arps += walk.notes.filter((x) => x.voice === v && x.tie).length;
    }
  });
}
console.log(`  ${compared} gate runs' start and end compared (${truncated} held across the loop point, ${ties} tied notes inside them); ${calls} calls' pitch compared with the register, ${wrong} differ, ${glided} skipped (a slide or vibrato)`);
assert.ok(wrong <= calls / WRONG_PITCH_AT_MOST_ONE_IN, `${wrong} of ${calls} calls sound another pitch than the walk's`);
if (songFiles.length >= SONGS_WITH_GT2_EXAMPLES) assert.ok(compared > 17000 && calls > 100000 && ties > 20000, "the comparison is big enough to mean something");
else assert.ok(compared > 0);

function readVlq(b, at) {
  let v = 0;
  for (;;) {
    const x = b[at++];
    v = (v << 7) | (x & 0x7f);
    if (!(x & 0x80)) return [v, at];
  }
}
function readSmf(b) {
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const tag = (at) => String.fromCharCode(...b.subarray(at, at + 4));
  assert.equal(tag(0), "MThd");
  assert.equal(dv.getUint32(4), 6);
  const format = dv.getUint16(8), ntrk = dv.getUint16(10), ppq = dv.getUint16(12);
  assert.ok(ppq > 0 && ppq < 0x8000, "a metrical division");
  let at = 14;
  const tracks = [];
  for (let t = 0; t < ntrk; t++) {
    assert.equal(tag(at), "MTrk");
    const len = dv.getUint32(at + 4);
    let p = at + 8;
    const stop = p + len;
    assert.ok(stop <= b.length, "the track fits the file");
    const ev = [];
    let tick = 0, ended = false;
    while (p < stop) {
      let d;
      [d, p] = readVlq(b, p);
      tick += d;
      const status = b[p++];
      if (status === 0xff) {
        const type = b[p++];
        let l;
        [l, p] = readVlq(b, p);
        ev.push({ tick, kind: "meta", type, data: b.subarray(p, p + l) });
        p += l;
        if (type === 0x2f) { ended = true; assert.equal(p, stop, "the end of track is last"); }
      } else {
        assert.ok(status >= 0x80 && status < 0xf0, `channel status ${status}`);
        const hi = status & 0xf0;
        assert.ok(hi === 0x80 || hi === 0x90, "only notes");
        ev.push({ tick, kind: hi === 0x90 ? "on" : "off", ch: status & 15, key: b[p], vel: b[p + 1] });
        assert.ok(b[p] < 128 && b[p + 1] < 128, "data bytes");
        p += 2;
      }
    }
    assert.ok(ended && p === stop);
    tracks.push(ev);
    at = stop;
  }
  assert.equal(at, b.length, "no bytes after the last track");
  return { format, ppq, tracks };
}

await test("the .mid: structure, one channel a voice, ticks are exact player calls, one constant tempo", () => {
  for (const stem of showcase) {
    const { path } = songFiles.find((f) => f.stem === stem);
    const r = songToMidi(readFileSync(path));
    const smf = readSmf(r.midi.bytes);
    assert.equal(smf.format, 1);
    assert.equal(smf.tracks.length, 4);
    assert.equal(smf.ppq, r.midi.ppq);
    const tempos = smf.tracks[0].filter((e) => e.kind === "meta" && e.type === 0x51);
    assert.equal(tempos.length, 1, "one tempo event: the tick-to-time map never changes");
    const us = (tempos[0].data[0] << 16) | (tempos[0].data[1] << 8) | tempos[0].data[2];
    assert.equal(us, r.midi.tempoUs);
    assert.equal(r.midi.beatCalls * r.midi.ticksPerCall, smf.ppq);
    const callUs = callRate(r.song.options).us;
    for (let v = 0; v < 3; v++) {
      const ev = smf.tracks[v + 1].filter((e) => e.kind === "on" || e.kind === "off");
      assert.ok(ev.every((e) => e.ch === v), "voice n is MIDI channel n - 1");
      const mine = r.walk.notes.filter((x) => x.voice === v);
      const ons = ev.filter((e) => e.kind === "on"), offs = ev.filter((e) => e.kind === "off");
      assert.equal(ons.length, mine.length);
      assert.equal(offs.length, mine.length);
      const seconds = (tick) => (tick * us) / smf.ppq;
      for (const [i, x] of mine.entries()) {
        const on = ons.find((e) => e.tick === (x.start - r.walk.origin) * r.midi.ticksPerCall && e.key === x.pitch);
        assert.ok(on, `note ${i} on`);
        assert.ok(Math.abs(seconds(on.tick) - (x.start - r.walk.origin) * callUs) < 1);
        assert.ok(offs.some((e) => e.tick === (x.end - r.walk.origin) * r.midi.ticksPerCall && e.key === x.pitch), `note ${i} off`);
      }
      const open = new Map();
      for (const e of ev) {
        if (e.kind === "on") { assert.ok(!open.has(e.key), "no double on"); open.set(e.key, e.tick); }
        else { assert.ok(open.has(e.key), "an off has its on"); assert.ok(e.tick >= open.get(e.key)); open.delete(e.key); }
      }
      assert.equal(open.size, 0);
    }
  }
});

await test("the tempo: CHROME COMET is 150 BPM, NEON HIGHWAY 125, and the beats are four rows", () => {
  const bpm = (stem) => songToMidi(readFileSync(songFiles.find((f) => f.stem === stem).path)).midi.bpm;
  assert.equal(bpm("chrome-comet"), 150);
  assert.equal(bpm("neon-highway"), 125);
  assert.equal(Math.round(bpm("iron-rite")), 107);
});

await test("timing options: NTSC and multispeed set the call rate; 25 Hz is every other frame", () => {
  assert.deepEqual(callRate({ multispeed: 1, ntsc: false }), { hz: 50, us: 20000 });
  assert.equal(callRate({ multispeed: 2, ntsc: false }).us, 10000);
  assert.equal(callRate({ multispeed: 0, ntsc: false }).us, 40000);
  assert.equal(Math.round(callRate({ multispeed: 1, ntsc: true }).us), 16667);
});

function build({ orders, instruments = [], wave = [], speed = [], patterns, opts = null }) {
  const out = [..."GTS5"].map((c) => c.charCodeAt(0));
  out.push(...new Array(96).fill(0), 1);
  for (const o of orders) {
    const entries = o.slice(0, -1);
    out.push(entries.length + 1, ...entries, 0xff, o[o.length - 1]);
  }
  out.push(instruments.length);
  for (const i of instruments) {
    out.push(i.ad ?? 0x09, i.sr ?? 0xf0, i.wavePtr ?? 1, 0, 0, 0, 0, i.gate ?? 2, i.first ?? 0x09, ...new Array(16).fill(0));
  }
  out.push(wave.length, ...wave.map((w) => w[0]), ...wave.map((w) => w[1]));
  out.push(0, 0);
  out.push(speed.length, ...speed.map((w) => w[0]), ...speed.map((w) => w[1]));
  out.push(patterns.length);
  for (const p of patterns) out.push(p.length + 1, ...p.flat(), 0xff, 0, 0, 0);
  if (opts) {
    const chunk = [..."OPTS"].map((c) => c.charCodeAt(0));
    const data = [1, 0, 0x0f, opts.multispeed, opts.ntsc ? 1 : 0, ...new Array(18).fill(0)];
    const body = [0x53, 0x49, 0x44, 0x4f, 0x55, 0x2d, 0x58, 0, 1, 0, 0, 0, 0, 0, 3, ...new Array(240).fill(0), 0, 0, 0, 0,
      ...chunk, data.length, 0, 0, 0, ...data, 0, 0, 0, 0];
    body[10] = body.length;
    body[11] = body.length >> 8;
    out.push(...body);
  }
  return Uint8Array.from(out);
}
const fromC0 = (semitones) => FIRSTNOTE + semitones;
const plain = [{ ad: 0x09, sr: 0xf0, gate: 2 }];
const ones = (len) => Array.from({ length: len }, () => [0xbd, 0, 0, 0]);
const sec = (b, sub = 0) => walkSong(parseSong(b), sub);

const first = (w, voice = 0) => w.notes.filter((x) => x.voice === voice).map((x) => [x.start - w.origin, x.end - w.origin, x.pitch, x.tie]);
const lead = (w) => w.ends.map((e) => e - w.origin);

await test("speed, tempo commands and funktempo set a row's length in calls", () => {
  const rows = [[fromC0(24), 1, 0, 0], [0xbd, 0, 0, 0], [fromC0(26), 1, 0xf, 0x04], [0xbd, 0, 0, 0], [fromC0(28), 1, 0, 0], [0xbd, 0, 0, 0]];
  const b = build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: [[0x41, 0x80], [0xff, 0]], patterns: [rows, ones(6)] });
  const w = sec(b);
  assert.deepEqual(first(w).map((x) => x[0]), [0, 12, 20]);
  assert.deepEqual(lead(w), [28, 28, 28], "6 + 6 + 4 + 4 + 4 + 4: the global tempo moves every voice");
  const f = [[fromC0(24), 1, 0xe, 0x01], [0xbd, 0, 0, 0], [fromC0(26), 1, 0, 0], [0xbd, 0, 0, 0]];
  const w2 = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: [[0x41, 0x80], [0xff, 0]], speed: [[3, 9]], patterns: [f, ones(4)] }));
  assert.deepEqual(first(w2).map((x) => x[0]), [0, 12]);
  assert.deepEqual(lead(w2), [24, 24, 24], "3 + 9 + 3 + 9");
  const own = [[fromC0(24), 1, 0xf, 0x83], [0xbd, 0, 0, 0], [fromC0(26), 1, 0, 0], [0xbd, 0, 0, 0]];
  const w3 = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: [[0x41, 0x80], [0xff, 0]], patterns: [own, ones(4)] }));
  assert.deepEqual(first(w3).map((x) => x[0]), [0, 6]);
  assert.deepEqual(lead(w3), [12, 24, 24], "voice 1 at 3 calls a row, the others at 6");
});

await test("orderlist repeats, transposes and the restart position", () => {
  const p0 = [[fromC0(12), 1, 0, 0], [0xbd, 0, 0, 0]];
  const p1 = [[fromC0(24), 1, 0, 0], [0xbd, 0, 0, 0]];
  const b = build({ orders: [[0, 0xf7, 0xd2, 1, 1], [0, 0], [0, 0]], instruments: plain, wave: [[0x41, 0x80], [0xff, 0]], patterns: [p0, p1] });
  const w = sec(b);
  assert.deepEqual(first(w).map((x) => [x[0], x[2]]), [[0, 24], [12, 43], [24, 43], [36, 43]]);
  assert.deepEqual(lead(w), [48, 12, 12], "a voice stops where its own orderlist comes round");
});

await test("key off, key on, hard restart: where a note ends", () => {
  const rows = [[fromC0(24), 1, 0, 0], [0xbd, 0, 0, 0], [0xbe, 0, 0, 0], [0xbd, 0, 0, 0], [fromC0(26), 1, 0, 0], [fromC0(28), 1, 0, 0], [0xbf, 0, 0, 0], [0xbd, 0, 0, 0]];
  const w = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: [{ gate: 2 }], wave: [[0x41, 0x80], [0xff, 0]], patterns: [rows, ones(8)] }));
  assert.deepEqual(first(w).map((x) => x.slice(0, 3)), [
    [0, 10, 36],
    [24, 28, 38],
    [30, 46, 40],
  ], "key off read at 10 for the row at 12; the next note's hard restart read at 28 for the row at 30; key on at 34 only re-opens the gate, the loop's first note hard-restarts it at 46");
});

await test("wavetable arpeggios: each pitch step is a tied note, a delay holds the note, $80 changes nothing", () => {
  const arp = [[0x41, 0x00], [0x41, 0x04], [0x41, 0x07], [0x02, 0x80], [0x41, 0x0c], [0xff, 0x00]];
  const rows = [[fromC0(24), 1, 0, 0], [0xbd, 0, 0, 0]];
  const w = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: arp, patterns: [rows, ones(2)] }));
  assert.deepEqual(first(w), [[0, 2, 36, false], [2, 3, 40, true], [3, 7, 43, true], [7, 10, 48, true]]);
  assert.equal(gateRuns(w.notes.filter((x) => x.voice === 0)).length, 1, "one gate, four notes");
  const hold = [[0x41, 0x05], [0x41, 0x80], [0x41, 0x00], [0xff, 0x00]];
  const w2 = sec(build({ orders: [[0xf2, 0, 0], [1, 0], [1, 0]], instruments: plain, wave: hold, patterns: [[[fromC0(12), 1, 0, 0], [0xbd, 0, 0, 0]], ones(2)] }));
  assert.deepEqual(first(w2).slice(0, 2), [[0, 3, 31, false], [3, 10, 26, true]]);
});

await test("a fixed-note step ($81 and up) names the note itself; a wavetable command's column is not a note", () => {
  const rows = [[fromC0(10), 1, 0, 0], [0xbd, 0, 0, 0]];
  const fixed = [[0x81, 0xa4], [0xff, 0x00]];
  const w = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: fixed, patterns: [rows, ones(2)] }));
  assert.deepEqual(first(w), [[0, 10, MIDI_GT2_C0 + 0x24, false]], "the row's note is not what sounds, and the note is not cut at call 1");
  const cmd = [[0x41, 0x00], [0xf7, 0x41], [0xff, 0x00]];
  const w2 = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: cmd, patterns: [rows, ones(2)] }));
  assert.deepEqual(first(w2), [[0, 10, MIDI_GT2_C0 + 10, false]]);
});

await test("a legato instrument ties notes; tone portamento changes pitch without a new gate", () => {
  const w = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: [{ gate: 0x42 }], wave: [[0x41, 0x80], [0xff, 0]],
    patterns: [[[fromC0(24), 1, 0, 0], [fromC0(26), 1, 0, 0], [fromC0(28), 1, 0, 0], [0xbe, 0, 0, 0]], ones(4)] }));
  assert.deepEqual(first(w), [[0, 6, 36, false], [6, 12, 38, true], [12, 16, 40, true]]);
  const tp = sec(build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: [[0x41, 0x80], [0xff, 0]], speed: [[0, 0]],
    patterns: [[[fromC0(24), 1, 0, 0], [fromC0(31), 1, 3, 0x00], [0xbd, 0, 0, 0], [0xbd, 0, 0, 0]], ones(4)] }));
  assert.deepEqual(first(tp), [[0, 6, 36, false], [6, 22, 43, true]]);
});

await test("a GoatTracker Stereo song gets six voices, six MIDI channels", () => {
  const { path } = songFiles.find((f) => f.stem === "chrome-comet");
  const b = readFileSync(path);
  const lists = [];
  let at = 101;
  for (let i = 0; i < 3; i++) { const len = b[at] + 2; lists.push(b.subarray(at, at + len)); at += len; }
  const stereo = Uint8Array.from([...b.subarray(0, 101), ...lists.flatMap((l) => [...l]), ...lists.flatMap((l) => [...l]), ...b.subarray(at)]);
  const mono = songToMidi(b);
  const st = songToMidi(stereo);
  assert.equal(st.song.channels, 6);
  assert.equal(st.walk.channels, 6);
  for (let v = 0; v < 3; v++) {
    const key = (x) => [x.start, x.end, x.pitch];
    assert.deepEqual(st.walk.notes.filter((x) => x.voice === v).map(key), mono.walk.notes.filter((x) => x.voice === v).map(key));
    assert.deepEqual(st.walk.notes.filter((x) => x.voice === v + 3).map(key), mono.walk.notes.filter((x) => x.voice === v).map(key));
  }
  const smf = readSmf(st.midi.bytes);
  assert.equal(smf.tracks.length, 7);
  assert.deepEqual([...new Set(smf.tracks.flatMap((t) => t.filter((e) => e.kind === "on").map((e) => e.ch)))].sort((a, b) => a - b), [0, 1, 2, 3, 4, 5]);
});

await test("SIDOU's OPTS chunk (multispeed, NTSC) scales the time", () => {
  const rows = [[fromC0(24), 1, 0, 0], [0xbd, 0, 0, 0]];
  const make = (opts) => build({ orders: [[0, 0], [1, 0], [1, 0]], instruments: plain, wave: [[0x41, 0x80], [0xff, 0]], patterns: [rows, ones(2)], opts });
  const base = songToMidi(make(null));
  const fast = songToMidi(make({ multispeed: 2, ntsc: false }));
  const ntsc = songToMidi(make({ multispeed: 1, ntsc: true }));
  assert.equal(base.seconds, 12 / 50);
  assert.equal(fast.song.options.multispeed, 2);
  assert.ok(Math.abs(fast.seconds - 24 / 100) < 1e-9, "twice the calls a second, twice the calls a row");
  assert.ok(ntsc.song.options.ntsc && Math.abs(ntsc.seconds - 12 / 60) < 1e-9);
});

await test("files that are not songs are refused with a sentence", () => {
  const msg = (b) => assert.throws(() => parseSong(b), SongError);
  msg(new Uint8Array(10));
  msg(Uint8Array.from([..."GTS!", ...new Array(200).fill(0)].map((c) => (typeof c === "string" ? c.charCodeAt(0) : c))));
  msg(Uint8Array.from([..."RIFF", ...new Array(200).fill(0)].map((c) => (typeof c === "string" ? c.charCodeAt(0) : c))));
  const cut = readFileSync(songFiles[0].path).subarray(0, 300);
  assert.throws(() => parseSong(cut), SongError);
});

console.log(`${n} tests passed`);
