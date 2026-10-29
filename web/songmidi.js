// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// A GoatTracker 2 song (.sng, SIDOU's song file; docs/reference/song-format.md) as a Standard MIDI
// File, for the Songs -> MIDI page. No DOM in here: web/songmidi.test.mjs runs it in Node and compares
// every note it finds with what GoatTracker's own packed player does (host/crates/gt2-ref).
//
// The walk (walkSong) follows GoatTracker 2.77's playroutine (player.s, as gplay.c's editor player
// has it) call by call: each channel's tick counter and tempo (speed, funktempo, the F and E
// commands, channel-specific tempo), the sequencer (orderlist repeats, transposes, the loop), the
// pattern fetch gatetimer frames before a row starts (which is when a hard restart or a key off
// closes the previous note), the instruments' first-frame waveform and their legato and
// no-hard-restart flags. A note is the row's note plus the orderlist transpose. The wavetable's
// note column moves it: a step with a relative note ($00-$7F, added to the row's note) or a fixed
// one ($81-$FF, the note itself) sets the pitch, and when that differs from the sounding note's the
// file ends it and starts the next in the same call (an arpeggio is a run of tied notes). Vibrato,
// portamento and the other pattern effects do not move it.
//
// A note is a stretch with the SID's gate bit high, as the player sets it: the gate flag (pattern
// key on and off, the hard restart before a note, the instrument's first-frame waveform) and the
// waveform byte (the wavetable's and the 7xx and 8xx commands'), so a drum whose wavetable drops the
// gate is as short as it sounds.
//
// Time is counted in player calls: call 0 is the first play call after init, which only sets the
// voices up. A call is 1/50 s on PAL, 1/60 on NTSC, divided by the multispeed (multispeed 0 is
// 25 Hz: 1/25 s).

export const FIRSTNOTE = 0x60, REST = 0xbd, KEYOFF = 0xbe, KEYON = 0xbf, ENDPATT = 0xff;
const CMD_TONEPORTA = 3, CMD_SETWAVE = 7, CMD_SETWAVEPTR = 8, CMD_FUNKTEMPO = 0xe, CMD_SETTEMPO = 0xf;
const LOOPSONG = 0xff, REPEAT = 0xd0, TRANSDOWN = 0xe0, TRANSUP = 0xf0;
const MAX_FRAMES = 60 * 60 * 50 * 4; // four minutes of 60 Hz at 4x, a hang guard
const MAX_INSTR = 64;

export class SongError extends Error {
  constructor(message) {
    super(message);
    this.name = "SongError";
  }
}

const text = (b, at, n) => {
  let s = "";
  for (let i = 0; i < n && b[at + i]; i++) s += String.fromCharCode(b[at + i]);
  return s;
};

// ---------------------------------------------------------------------------------------------
// Reading the file

// One read of the GT2 part with `channels` orderlists a subtune. Returns the song and the offset
// where the part ends, or null if the file is too short or its counts are out of range.
function readGt2(b, channels) {
  const ident = text(b, 0, 4);
  let at = 4 + 96;
  const byte = () => {
    if (at >= b.length) throw new RangeError("short");
    return b[at++];
  };
  const take = (n) => {
    if (at + n > b.length) throw new RangeError("short");
    const s = b.subarray(at, at + n);
    at += n;
    return s;
  };
  try {
    const subtunes = byte();
    if (subtunes < 1 || subtunes > 32) return null;
    const orders = [];
    for (let s = 0; s < subtunes; s++) {
      const row = [];
      for (let c = 0; c < channels; c++) {
        const n = byte();
        const o = Uint8Array.from(take(n + 1));
        if (n < 2 || o[n - 1] !== LOOPSONG) return null; // entries, then the $FF, then the restart
        row.push(o);
      }
      orders.push(row);
    }
    const count = byte();
    if (count > 63) return null;
    const instruments = new Array(MAX_INSTR).fill(null);
    for (let i = 1; i <= count; i++) {
      const r = take(25);
      let gate = r[7];
      let first = r[8];
      // GTS3 and GTS4 kept legato and no-hard-restart in the first-frame waveform (gsong.c)
      if (ident < "GTS5") {
        if (first >= 0x80) {
          gate |= 0x80;
          first &= 0x7f;
        }
        if (!first) gate |= 0x40;
      }
      instruments[i] = { ad: r[0], sr: r[1], wavePtr: r[2], pulsePtr: r[3], filtPtr: r[4], speedPtr: r[5],
        vibDelay: r[6], gateTimer: gate, firstWave: first, name: text(r, 9, 16) };
    }
    const tables = [];
    for (let t = 0; t < 4; t++) {
      const rows = byte();
      tables.push({ left: Uint8Array.from(take(rows)), right: Uint8Array.from(take(rows)) });
    }
    const pcount = byte();
    if (pcount < 1 || pcount > 208) return null;
    const patterns = [];
    for (let p = 0; p < pcount; p++) {
      const rows = byte();
      if (rows < 1 || rows > 129) return null;
      patterns.push(Uint8Array.from(take(rows * 4)));
    }
    return {
      end: at,
      song: { ident, name: text(b, 4, 32), author: text(b, 36, 32), copyright: text(b, 68, 32), channels,
        subtunes: orders, instruments, tables, patterns },
    };
  } catch (e) {
    if (e instanceof RangeError) return null;
    throw e;
  }
}

// GoatTracker Stereo's determinechannels (docs/reference/song-format.md "Mono or stereo"): trial-read
// 6 x S orderlists; stereo only if every one passes.
function guessStereo(b) {
  let at = 101;
  const subtunes = b[100];
  for (let i = 0; i < subtunes * 6; i++) {
    const n = b[at];
    if (!n) return false;
    if ((b[at + n + 1] ?? 0) !== 0xff) return false;
    if ((b[at + n + 2] ?? 0) >= n + 1) return false;
    at += n + 3;
    if (at > b.length) return false;
  }
  return true;
}

const MAGIC = [0x53, 0x49, 0x44, 0x4f, 0x55, 0x2d, 0x58, 0x00];

// The trailer's chunks (docs/reference/song-format.md "SIDOU's trailer"), if one starts at `at`:
// { channels, chunks: Map(tag -> data) }. Its CRCs are not checked: only OPTS, MIDI and the
// channel count are read, and a damaged one cannot do more than give a wrong tempo.
function readTrailer(b, at) {
  if (at + 263 > b.length || MAGIC.some((m, i) => b[at + i] !== m)) return null;
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  if (dv.getUint16(at + 8, true) !== 1) return null;
  const len = dv.getUint32(at + 10, true);
  if (len < 263 || at + len > b.length) return null;
  const chunks = new Map();
  let p = at + 259;
  while (p + 8 <= at + len - 4) {
    const tag = text(b, p, 4);
    const d = dv.getUint32(p + 4, true);
    if (d < 1 || p + 8 + d > at + len - 4) break;
    if (!chunks.has(tag)) chunks.set(tag, b.subarray(p + 8, p + 8 + d));
    p += 8 + d;
  }
  return { channels: b[at + 14], chunks };
}

// Reads a .sng (GTS3-GTS5, mono or GoatTracker Stereo) and the parts of SIDOU's trailer that decide
// the timing. Throws SongError.
export function parseSong(bytes) {
  const b = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  if (b.length < 101) throw new SongError("This is not a GoatTracker 2 song (the file is too short).");
  const ident = text(b, 0, 4);
  if (ident === "GTS!") throw new SongError("This is a GoatTracker 1 song; open it in GoatTracker 2 and save it again.");
  if (ident === "GTS2") throw new SongError("This is an old three-table GTS2 song; open it in GoatTracker 2 and save it again.");
  if (!["GTS3", "GTS4", "GTS5"].includes(ident)) throw new SongError("This is not a GoatTracker 2 song (.sng).");
  const guess = guessStereo(b) ? 6 : 3;
  // Both readings, as SIDOU does: the one that ends at a trailer that records its channel count,
  // then at any trailer, then at the file's end, wins; a tie goes to the guess.
  let best = null;
  for (const channels of [guess, guess === 6 ? 3 : 6]) {
    const r = readGt2(b, channels);
    if (!r) continue;
    const trailer = readTrailer(b, r.end);
    const score = trailer ? (trailer.channels === channels ? 3 : trailer.channels === 0 ? 2 : 1) : r.end === b.length ? 1 : 0;
    if (!best || score > best.score) best = { ...r, trailer, score };
  }
  if (!best) throw new SongError("This song file is damaged or cut short.");
  const song = best.song;
  const opts = best.trailer?.chunks.get("OPTS");
  const midi = best.trailer?.chunks.get("MIDI");
  song.options = {
    multispeed: opts && opts[0] === 1 && opts.length >= 5 ? Math.min(opts[3], 16) : 1,
    ntsc: !!(opts && opts[0] === 1 && opts.length >= 5 && opts[4] === 1),
  };
  song.rowsPerBeat = midi && midi[0] === 1 && midi.length >= 3 && midi[2] >= 1 && midi[2] <= 16 ? midi[2] : 4;
  return song;
}

// ---------------------------------------------------------------------------------------------
// The walk

function rowsOf(song, num) {
  // a pattern the file lacks is 64 empty rows, as GoatTracker's loader leaves one
  const p = song.patterns[num];
  const rows = [];
  if (!p) return Array.from({ length: 64 }, () => [REST, 0, 0, 0]);
  for (let i = 0; i < p.length; i += 4) {
    if (p[i] === ENDPATT) break;
    rows.push([p[i], p[i + 1], p[i + 2], p[i + 3]]);
  }
  return rows;
}

// Walks one subtune call by call until every voice has played its orderlist once. Returns
//   notes   [{ voice, start, end, pitch, tie, instr }] in player calls from the first call, pitch a MIDI
//           note number (GT2's C-0 is 12); tie: it began without the gate falling (legato
//           instrument, tone portamento, a wavetable step that changes the pitch): the note before
//           it ended in the same call
//   ends    each voice's call where its orderlist came round again (the walk stops there)
//   origin  the call of the first row
//   tempo   [{ frame, kind: "speed"|"funk", text }] changes
//   rows    each voice's row count in its pass
export function walkSong(song, subtune = 0) {
  const orders = song.subtunes[subtune];
  if (!orders) throw new SongError(`The song has no subtune ${subtune + 1}.`);
  const ncha = orders.length;
  const mult = song.options.multispeed;
  const ins = (n) => song.instruments[n] ?? { gateTimer: 0, firstWave: 0 };
  const rowCache = new Map();
  const rowsFor = (n) => {
    if (!rowCache.has(n)) rowCache.set(n, rowsOf(song, n));
    return rowCache.get(n);
  };
  for (const o of orders) {
    if (o.indexOf(LOOPSONG) <= 0) throw new SongError("A voice's orderlist is empty.");
  }
  // 18 of the 63 instruments' AD in the last slot is GoatTracker's default speed (greloc.c)
  const last = song.instruments[MAX_INSTR - 1];
  const defaultTempo = last && last.ad >= 2 && !last.wavePtr ? last.ad - 1 : (mult ? mult * 6 - 1 : 5);
  // gt2reloc's packed player starts the funktempo pair at 8 and 5 calls (mt_funktempotbl); the
  // counter value is one less than the call count
  const funk = [8 - 1, 5 - 1];
  const tempoLog = [];
  const chn = orders.map((order, c) => ({
    c, order, songptr: 0, pattnum: 0, rows: [], pattptr: -1, repeat: 0, trans: 0, instr: 1,
    tick: 1, tempo: defaultTempo, gatetimer: 0, newnote: null, newcmd: 0, newdata: 0,
    gate: 0, wave: 0, waveptr: 0, wavetime: 0, pitch: null, sound: null, cur: null, wrapSeq: null, end: null, tick0s: 0, firstRow: null, rowCount: 0,
  }));
  const notes = [];
  let origin = null;

  const closeNote = (ch, frame) => {
    if (!ch.cur) return;
    if (frame > ch.cur.start) {
      const { pending, ...note } = ch.cur;
      notes.push({ ...note, end: frame });
    }
    ch.cur = null;
  };
  const clampPitch = (p) => Math.max(0, Math.min(127, p));
  // `pending`: the note began in the call that set its row note, before the wavetable's first step
  // ran; that step (the next call's) names the pitch the voice sounds, with no change in between
  const openNote = (ch, frame, tie, pending = false) => {
    const pitch = ch.sound ?? clampPitch(12 + ch.pitch);
    ch.cur = { voice: ch.c, start: frame, pitch, tie, instr: ch.instr, pending };
  };

  const sequencer = (ch, frame) => {
    ch.pattptr = 0;
    const o = ch.order;
    if (o[ch.songptr] === LOOPSONG) {
      ch.songptr = o[ch.songptr + 1];
      if (ch.wrapSeq === null) ch.wrapSeq = frame;
      if (ch.songptr >= o.indexOf(LOOPSONG)) ch.songptr = 0;
    }
    if (o[ch.songptr] >= TRANSDOWN && o[ch.songptr] < LOOPSONG) {
      ch.trans = o[ch.songptr] - TRANSUP;
      ch.songptr++;
    }
    if (o[ch.songptr] >= REPEAT && o[ch.songptr] < TRANSDOWN) {
      ch.repeat = o[ch.songptr] - REPEAT;
      ch.songptr++;
    }
    ch.pattnum = o[ch.songptr];
    if (ch.repeat) ch.repeat--;
    else ch.songptr++;
    if (ch.pattnum >= 208) ch.pattnum = 0;
    ch.rows = rowsFor(ch.pattnum);
    if (ch.pattptr >= ch.rows.length) ch.pattptr = 0;
  };

  const fetch = (ch, frame) => {
    if (ch.pattptr >= ch.rows.length || ch.pattptr < 0) return; // an empty pattern
    const [note, instr, cmd, data] = ch.rows[ch.pattptr];
    if (instr) ch.instr = instr;
    ch.newcmd = cmd;
    ch.newdata = data;
    ch.pattptr++;
    if (ch.pattptr >= ch.rows.length) ch.pattptr = -1;
    if (note === KEYOFF) ch.gate = 0xfe;
    if (note === KEYON) ch.gate = 0xff;
    if (note >= FIRSTNOTE && note < REST) {
      ch.newnote = note + ch.trans;
      if (cmd !== CMD_TONEPORTA && !(ins(ch.instr).gateTimer & 0x40)) ch.gate = 0xfe;
    }
  };

  // The wavetable's step for this call (gplay.c WAVEEXEC): the waveform it sets decides whether
  // the gate bit is high, and its note column the pitch (the MIDI note it sets, or null); the
  // commands only move other registers, which the walk does not follow.
  const waveExec = (ch) => {
    if (!ch.waveptr) return null;
    const wt = song.tables[0];
    const left = wt.left[ch.waveptr - 1], right = wt.right[ch.waveptr - 1];
    if (left === undefined) return null;
    if (left > 0x0f) {
      if (left < 0xe0) ch.wave = left;
      else if (left <= 0xef) ch.wave = left & 0x0f;
      else if (left <= 0xfe && (left & 0x0f) === 7) ch.wave = right;
    } else if (ch.wavetime !== left) {
      ch.wavetime++;
      return null;
    }
    ch.wavetime = 0;
    ch.waveptr++;
    if (wt.left[ch.waveptr - 1] === 0xff) ch.waveptr = wt.right[ch.waveptr - 1];
    if (left >= 0xf0 || right === 0x80) return null; // a command, or "no change"
    // an unsigned byte sum, as the player's: a relative note under the transpose wraps
    const note = (right < 0x80 ? (right + ch.pitch) & 0xff : right) & 0x7f;
    ch.sound = clampPitch(12 + note);
    return ch.sound;
  };

  const logTempo = (entry) => {
    if (!tempoLog.some((t) => t.frame === entry.frame && t.text === entry.text)) tempoLog.push(entry);
  };
  const setTempo = (frame, ch, data) => {
    let t = data & 0x7f;
    if (t >= 3) t--;
    const label = data & 0x7f;
    if (data >= 0x80) {
      ch.tempo = t;
      logTempo({ frame, kind: "speed", text: `voice ${ch.c + 1} speed ${label}` });
    } else {
      for (const x of chn) x.tempo = t;
      logTempo({ frame, kind: label < 2 ? "funk" : "speed", text: label < 2 ? `funk ${label}` : `speed ${label}` });
    }
  };

  const step = (ch, frame) => {
    ch.tick--;
    let skipWave = false;
    let retune = false;
    let sound = null;
    if (ch.tick === 0) {
      ch.tick0s++;
      if (ch.tick0s === 2) origin = origin === null ? frame : Math.min(origin, frame);
      if (ch.end === null && ch.wrapSeq !== null && frame > ch.wrapSeq) {
        ch.end = frame;
        closeNote(ch, frame);
      }
      if (ch.end === null && ch.tick0s >= 2) ch.rowCount++;
      if (ch.pattptr === -1) sequencer(ch, frame);
      ch.gatetimer = ins(ch.instr).gateTimer & 0x3f;
      if (ch.newnote !== null) {
        const i = ins(ch.instr);
        ch.pitch = ch.newnote - FIRSTNOTE;
        ch.sound = null;
        retune = true;
        if (ch.newcmd !== CMD_TONEPORTA) {
          if (i.firstWave) {
            if (i.firstWave >= 0xfe) ch.gate = i.firstWave;
            else {
              ch.wave = i.firstWave;
              ch.gate = 0xff;
            }
          }
          ch.waveptr = i.wavePtr ?? 0;
        }
      }
      if (ch.newcmd === CMD_SETWAVE) {
        ch.wave = ch.newdata;
      } else if (ch.newcmd === CMD_SETWAVEPTR) {
        ch.waveptr = ch.newdata;
        ch.wavetime = 0;
      } else if (ch.newcmd === CMD_FUNKTEMPO) {
        const p = ch.newdata;
        // E00 leaves the pair as it is (the editor's player; the packed one reads a byte before
        // the speedtable)
        if (p && tables(song, 3, p)) {
          funk[0] = song.tables[3].left[p - 1] - 1;
          funk[1] = song.tables[3].right[p - 1] - 1;
        }
        for (const x of chn) x.tempo = 0;
        logTempo({ frame, kind: "funk", text: p ? `funk ${funk[0] + 1}/${funk[1] + 1}` : "funk" });
      } else if (ch.newcmd === CMD_SETTEMPO) {
        setTempo(frame, ch, ch.newdata);
      }
      if (ch.newnote !== null) {
        ch.newnote = null;
        if (ch.newcmd !== CMD_TONEPORTA) skipWave = true;
      }
    } else if (ch.tick < 0) {
      if (ch.tempo >= 2) ch.tick = ch.tempo;
      else {
        ch.tick = funk[ch.tempo];
        ch.tempo ^= 1;
      }
    }
    if (!skipWave) {
      sound = waveExec(ch);
      if (ch.tick === ch.gatetimer) fetch(ch, frame);
    }
    // the SID's gate bit: the waveform byte and the gate flag both
    if (ch.end === null) {
      if ((ch.wave & ch.gate & 1) === 1) {
        if (!ch.cur) openNote(ch, frame, false, retune);
        else if (retune) {
          closeNote(ch, frame);
          openNote(ch, frame, true, true);
        } else if (sound !== null && sound !== ch.cur.pitch) {
          if (ch.cur.pending) ch.cur.pitch = sound;
          else {
            closeNote(ch, frame);
            openNote(ch, frame, true);
          }
        }
        if (ch.cur && !skipWave) ch.cur.pending = false;
      } else {
        closeNote(ch, frame);
      }
    }
  };

  // call 0 only sets the channels up (mt_play's song initialisation returns before any channel
  // runs): the first tick 0 is call 1
  let frame = 1;
  for (; chn.some((ch) => ch.end === null); frame++) {
    if (frame > MAX_FRAMES) throw new SongError("The song runs for too long without coming round again.");
    for (const ch of chn) step(ch, frame);
  }
  notes.sort((a, b) => a.start - b.start || a.voice - b.voice);
  return {
    notes,
    ends: chn.map((ch) => ch.end),
    origin: origin ?? 0,
    tempo: tempoLog.filter((t) => t.frame <= Math.max(...chn.map((ch) => ch.end))),
    rows: chn.map((ch) => ch.rowCount),
    channels: ncha,
  };
}

const tables = (song, t, p) => p >= 1 && p <= song.tables[t].left.length;

// ---------------------------------------------------------------------------------------------
// The MIDI file

// Microseconds in one player call, and calls a second.
export function callRate(options) {
  const base = options.ntsc ? 60 : 50;
  const hz = options.multispeed ? base * options.multispeed : base / 2;
  return { hz, us: 1e6 / hz };
}

const vlq = (n) => {
  const out = [n & 0x7f];
  while ((n >>= 7)) out.unshift((n & 0x7f) | 0x80);
  return out;
};
const u32 = (n) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
const ascii = (s) => [...s].map((ch) => (ch.charCodeAt(0) < 128 ? ch.charCodeAt(0) : 63));
const meta = (type, data) => [0xff, type, ...vlq(data.length), ...data];

function track(events) {
  // events: { tick, order, bytes }; stable by time, then order (offs before ons)
  const sorted = events.map((e, i) => ({ ...e, i })).sort((a, b) => a.tick - b.tick || a.order - b.order || a.i - b.i);
  const body = [];
  let last = 0;
  for (const e of sorted) {
    body.push(...vlq(e.tick - last), ...e.bytes);
    last = e.tick;
  }
  body.push(...vlq(0), ...meta(0x2f, []));
  return [...ascii("MTrk"), ...u32(body.length), ...body];
}

// The MIDI file of a walk: format 1, a conductor track and one track a voice (MIDI channel =
// voice, 0-2, or 0-5 for a GoatTracker Stereo song). One MIDI tick is 1/20 of a player call
// (less if the beat would need more than the 32767 a header can hold), so every call, and so
// every note start and end the player makes, lands on a whole tick and the file plays back in
// the player's own time with one constant tempo: a beat is `rowsPerBeat` rows of the song's
// average row length, rounded to whole calls. Speed and funktempo changes are markers in the
// conductor track, not tempo events, which would move every later tick.
export function toMidi(song, walk, { name = song.name, rowsPerBeat = song.rowsPerBeat } = {}) {
  const { us } = callRate(song.options);
  const rows = walk.rows.reduce((a, b) => a + b, 0);
  const span = walk.ends.reduce((a, e) => a + (e - walk.origin), 0);
  const avgRow = rows ? span / rows : 6;
  const beat = Math.max(1, Math.round(rowsPerBeat * avgRow));
  const perCall = Math.max(1, Math.min(20, Math.floor(32767 / beat)));
  const ppq = Math.min(32767, beat * perCall);
  const tempoUs = Math.max(1, Math.min(0xffffff, Math.round(beat * us)));
  const at = (frame) => (frame - walk.origin) * perCall;

  const conductor = [
    { tick: 0, order: 0, bytes: meta(0x03, ascii(name || "SIDOU song")) },
    { tick: 0, order: 1, bytes: meta(0x51, [tempoUs >> 16, (tempoUs >> 8) & 255, tempoUs & 255]) },
    { tick: 0, order: 1, bytes: meta(0x58, [4, 2, 24, 8]) },
  ];
  for (const t of walk.tempo) {
    if (t.frame < walk.origin) continue;
    conductor.push({ tick: at(t.frame), order: 2, bytes: meta(0x06, ascii(t.text)) });
  }
  const end = Math.max(...walk.ends);
  conductor.push({ tick: at(end), order: 9, bytes: meta(0x06, ascii("loop")) });

  const tracks = [track(conductor)];
  for (let v = 0; v < walk.channels; v++) {
    const ev = [{ tick: 0, order: 0, bytes: meta(0x03, ascii(`Voice ${v + 1}`)) }];
    for (const n of walk.notes) {
      if (n.voice !== v) continue;
      ev.push({ tick: at(n.start), order: 5, bytes: [0x90 | v, n.pitch, 100] });
      ev.push({ tick: at(n.end), order: 4, bytes: [0x80 | v, n.pitch, 0] });
    }
    tracks.push(track(ev));
  }
  const header = [...ascii("MThd"), ...u32(6), 0, 1, 0, tracks.length, ppq >> 8, ppq & 255];
  const out = new Uint8Array(header.length + tracks.reduce((a, t) => a + t.length, 0));
  out.set(header);
  let p = header.length;
  for (const t of tracks) {
    out.set(t, p);
    p += t.length;
  }
  return { bytes: out, ppq, tempoUs, beatCalls: beat, bpm: 60e6 / tempoUs, ticksPerCall: perCall };
}

// One call: the page's whole job.
export function songToMidi(bytes, { subtune = 0, name } = {}) {
  const song = parseSong(bytes);
  const walk = walkSong(song, subtune);
  const midi = toMidi(song, walk, { name: name ?? song.name });
  const { hz } = callRate(song.options);
  const lastEnd = Math.max(...walk.ends);
  return { song, walk, midi, seconds: (lastEnd - walk.origin) / hz };
}
