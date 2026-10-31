// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The JavaScript side of SIDOU's wasm core (sidou-core.wasm, built by tools/wasm-core from
// core/song, core/player, core/sid and platform/web/core_api.c): the FM-1's own .sng reader
// and writer, GTI5 export and song player, in the browser and in Node. One song at a time;
// a file is refused here as the FM-1 refuses it, with the message its screen shows.
//   const core = await loadCore(await (await fetch("sidou-core.wasm")).arrayBuffer());
//   const song = core.read(bytes);   // throws CoreError with the device's message
//   core.write({ trailer: false })   // { bytes, warning }

export class CoreError extends Error {
  constructor(message, code = null) {
    super(message);
    this.name = "CoreError";
    this.code = code;
  }
}

// gt2_read_info.trailer
export const TRAILER = ["none", "read", "dropped", "locks-dropped"];
// gt2_err, for a refused instrument export
const GT2_ERR = ["OK", "RANGE", "ORDER RULE", "ORDER FULL", "ROWS", "PATTERNS", "IN USE", "FULL",
  "FRAGMENTED", "REGION", "PENDING", "SONG FULL", "SAVING"];
export const TABLE_NAMES = ["wave", "pulse", "filter", "speed"];
// GoatTracker's notes: C-0 is 0
const NOTE_NAMES = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"];
export const noteName = (n) => `${NOTE_NAMES[n % 12]}${Math.floor(n / 12)}`;
// A GoatTracker Stereo song is refused by the packer's player (GT2_PLAY_ERR_STEREO)
const PLAY_ERR = { "-1": "the subtune is not in the song", "-2": "a GoatTracker Stereo song (6 channels) has no preview yet", "-10": "multispeed above 4" };

// platform/web/core_api.c's sc_edit operations
export const EDIT = { INSTR: 0, TABLE: 1, ROW: 2, ORDER: 3, ORDER_INSERT: 4, ORDER_DELETE: 5, PUSH_ROW: 6, PULL_ROW: 7,
  INSERT_ROW: 8, DELETE_ROW: 9, RESIZE: 10, PATTERN_ADD: 11, PATTERN_REMOVE: 12 };
// What each refusal means (gt2_err), the FM-1's own word first where its screen has one
// (app/song_edit.c: SONG FULL, INVALID EDIT).
export const EDIT_ERR = {
  1: "INVALID EDIT: out of range",
  2: "INVALID EDIT: a transpose cannot follow a repeat, and an orderlist ends with a pattern",
  3: "INVALID EDIT: an orderlist holds at most 254 entries",
  4: "INVALID EDIT: a pattern has 1 to 128 rows",
  5: "INVALID EDIT: the song has all 208 patterns",
  6: "INVALID EDIT: an orderlist plays this pattern",
  7: "SONG FULL: no room left in the song's 64 KiB of memory",
  8: "TIDYING: the song's memory is too fragmented",
  11: "SONG FULL: the song as the FM-1 saves it would be over 64 KiB",
};
export class EditError extends Error {
  constructor(code) {
    super(EDIT_ERR[code] ?? `INVALID EDIT (${code})`);
    this.name = "EditError";
    this.code = code;
  }
}
// A lock's parameter (song_ext.h's SONG_LOCK_*)
export const LOCK_NAMES = ["CUTOFF", "RESONANCE", "FILTER MODE", "ROUTE", "PULSE", "AGE", "DAC", "THUMP", "BLEED", "CHANCE"];

const text = (mem, ptr, max) => {
  const b = new Uint8Array(mem.buffer, ptr, max);
  const end = b.indexOf(0);
  return String.fromCharCode(...b.subarray(0, end < 0 ? max : end));
};

export async function loadCore(source) {
  const bytes = source instanceof ArrayBuffer || ArrayBuffer.isView(source) ? source : await (await fetch(source)).arrayBuffer();
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Core(instance.exports);
}

export class Core {
  constructor(x) {
    this.x = x;
    this.loaded = false;
  }

  get mem() { return this.x.memory; }

  info() {
    const p = this.x.sc_info();
    const v = new DataView(this.mem.buffer, p + 32, 32);
    const w = (i) => v.getUint32(i * 4, true);
    return {
      message: text(this.mem, p, 32), result: v.getInt32(0, true), version: w(1), channels: w(2),
      trailer: TRAILER[w(3)] ?? String(w(3)), gt2Bytes: w(4), fileBytes: w(5), needBytes: w(6), savedBytes: w(7),
    };
  }

  // Reads a .sng into the core. Resolves to the read's info ({ version, channels, trailer,
  // message: a warning such as SNG: EXTRAS DROPPED, or "" }); a refused file throws a CoreError
  // whose message is the device's (SNG: BAD PATTERNS, SNG: TOO BIG (70K OF 64K), ...).
  read(bytes) {
    if (bytes.length > this.x.sc_in_max()) throw new CoreError("SNG: TOO BIG");
    new Uint8Array(this.mem.buffer, this.x.sc_in(), bytes.length).set(bytes);
    const r = this.x.sc_read(bytes.length);
    const info = this.info();
    this.loaded = r === 0;
    if (r !== 0) throw new CoreError(info.message || `SNG: REFUSED (${r})`, r);
    return info;
  }

  #need() {
    if (!this.loaded) throw new CoreError("no song loaded");
  }

  // The song as a .sng: with SIDOU's trailer (the FM-1's own save, the default), or plain
  // GoatTracker. A plain file is read back as GoatTracker reads it; `warning` says when that gives
  // another channel count (SNG: READS BACK AS STEREO / MONO: only the trailer told them apart).
  write({ trailer = true } = {}) {
    this.#need();
    const n = this.x.sc_write(trailer ? 1 : 0);
    if (n < 0) throw new CoreError("SNG: WRITE FAILED");
    const bytes = new Uint8Array(this.mem.buffer, this.x.sc_out(), n).slice();
    let warning = "";
    if (!trailer) {
      const c = this.x.sc_check_plain();
      if (c !== 0) warning = text(this.mem, this.x.sc_info(), 32) || "SNG: SAVED FILE UNREADABLE";
    }
    return { bytes, warning };
  }

  // Instrument i (1-63) as a GoatTracker GTI5 instrument file, its tables with it.
  exportInstrument(i) {
    this.#need();
    const n = this.x.sc_export_instrument(i);
    if (n < 0) throw new CoreError(`instrument ${i} cannot be exported (${GT2_ERR[-n] ?? -n})`, -n);
    return new Uint8Array(this.mem.buffer, this.x.sc_out(), n).slice();
  }

  // The loaded song for display: strings, instruments, tables, orderlists and patterns.
  song() {
    this.#need();
    const x = this.x, mem = this.mem;
    const word = (i) => x.sc_song_word(i);
    const bytes = (p, n) => (p ? Array.from(new Uint8Array(mem.buffer, p, n)) : null);
    const subtunes = word(0), channels = word(1);
    const instruments = [];
    for (let i = 1; i <= word(2); i++) {
      const b = bytes(x.sc_instrument(i), 25);
      instruments.push({ index: i, name: String.fromCharCode(...b.slice(9)).replace(/\0[\s\S]*$/, ""), bytes: b.slice(0, 9) });
    }
    const tables = TABLE_NAMES.map((name, t) => {
      const rows = x.sc_table_rows(t);
      return { name, rows, left: bytes(x.sc_table(t, 0), rows), right: bytes(x.sc_table(t, 1), rows) };
    });
    const orders = [];
    for (let s = 0; s < subtunes; s++) {
      orders.push(Array.from({ length: channels }, (_, c) => {
        const n = x.sc_order_entries(s, c);
        const d = bytes(x.sc_order(s, c), n + 2);
        return d ? { entries: d.slice(0, n), restart: d[n + 1] } : { entries: [], restart: 0 };
      }));
    }
    const patterns = [];
    for (let p = 0; p < word(3); p++) {
      const rows = x.sc_pattern_rows(p);
      const d = bytes(x.sc_pattern(p), rows * 4);
      const cells = [];
      if (d) for (let r = 0; r + 1 < rows; r++) cells.push(d.slice(r * 4, r * 4 + 4));
      patterns.push({ index: p, held: !!d, rows: d ? rows - 1 : 64, cells });
    }
    return {
      name: text(mem, x.sc_song_string(0), 32), author: text(mem, x.sc_song_string(1), 32),
      copyright: text(mem, x.sc_song_string(2), 32), version: word(5), channels, subtunes,
      savedBytes: word(4), instruments, tables, orders, patterns,
    };
  }

  // ---- Edits: the FM-1's own primitives (gt2_song.h), so a refusal is the FM-1's. Each throws an
  // EditError (its code a gt2_err) and changes nothing then.

  edit(op, a = 0, b = 0, c = 0, d = 0) {
    this.#need();
    const r = this.x.sc_edit(op, a, b, c, d >>> 0);
    if (r !== 0) throw new EditError(r);
  }

  // Instrument i (1-63), byte field (0-8 the parameters, 9-24 the name).
  setInstrument(i, field, value) { this.edit(EDIT.INSTR, i, field, value); }
  // Table t (0 wave, 1 pulse, 2 filter, 3 speed), row 0-254, side 0 left or 1 right.
  setTable(t, row, side, value) { this.edit(EDIT.TABLE, t, row, side, value); }
  // A pattern row's four bytes: note, instrument, command, parameter.
  setRow(p, row, cell) { this.edit(EDIT.ROW, p, row, 0, cell[0] | (cell[1] << 8) | (cell[2] << 16) | (cell[3] << 24)); }
  // An orderlist entry (pos === entries: the restart position).
  setOrder(subtune, channel, pos, value) { this.edit(EDIT.ORDER, subtune, channel, pos, value); }
  insertOrder(subtune, channel, pos, value) { this.edit(EDIT.ORDER_INSERT, subtune, channel, pos, value); }
  deleteOrder(subtune, channel, pos) { this.edit(EDIT.ORDER_DELETE, subtune, channel, pos); }
  // TRACKER's INS and DEL: the pattern keeps its length (push drops the last row, pull adds an
  // empty one at the end); the rows' locks move with them.
  pushRow(p, row) { this.edit(EDIT.PUSH_ROW, p, row); }
  pullRow(p, row) { this.edit(EDIT.PULL_ROW, p, row); }
  // A row inserted or deleted, the pattern a row longer or shorter.
  insertRow(p, row) { this.edit(EDIT.INSERT_ROW, p, row); }
  deleteRow(p, row) { this.edit(EDIT.DELETE_ROW, p, row); }
  resizePattern(p, rows) { this.edit(EDIT.RESIZE, p, rows); }
  // A new empty pattern of `rows` rows: the lowest number free; returns it.
  addPattern(rows = 64) { this.edit(EDIT.PATTERN_ADD, rows); return this.x.sc_added(); }
  // A pattern no orderlist plays.
  removePattern(p) { this.edit(EDIT.PATTERN_REMOVE, p); }
  // The song as the FM-1 saves it, in bytes (trailer included); the cap is 65,536.
  get savedBytes() { return this.loaded ? this.x.sc_song_word(4) : 0; }

  // A row's parameter locks: [{ param, name, value }].
  locks(p, row) {
    this.#need();
    const n = this.x.sc_locks_row(p, row);
    const b = new Uint8Array(this.mem.buffer, this.x.sc_out(), n * 5);
    return Array.from({ length: n }, (_, k) => ({ param: b[k * 5 + 2], name: LOCK_NAMES[b[k * 5 + 2]] ?? "?", value: b[k * 5 + 3] | (b[k * 5 + 4] << 8) }));
  }

  // Instrument i (1-63): its 25 bytes, the name's 16 last (a free one is all zeros).
  instrument(i) {
    this.#need();
    return Array.from(new Uint8Array(this.mem.buffer, this.x.sc_instrument(i), 25));
  }
  // Table t's 255 rows, left and right, and how many the song saves.
  table(t) {
    this.#need();
    const side = (k) => Array.from(new Uint8Array(this.mem.buffer, this.x.sc_table(t, k), 255));
    return { rows: this.x.sc_table_rows(t), left: side(0), right: side(1) };
  }
  // Pattern p (0-207): null if the song does not hold it, else its rows (4 bytes each).
  pattern(p) {
    this.#need();
    const ptr = this.x.sc_pattern(p);
    if (!ptr) return null;
    const rows = this.x.sc_pattern_rows(p) - 1;
    const d = new Uint8Array(this.mem.buffer, ptr, rows * 4);
    return Array.from({ length: rows }, (_, r) => Array.from(d.subarray(r * 4, r * 4 + 4)));
  }
  // The patterns the song holds, by number.
  heldPatterns() {
    this.#need();
    const out = [];
    for (let p = 0; p < 208; p++) if (this.x.sc_pattern(p)) out.push(p);
    return out;
  }

  // The whole song's state, for an undo, and back.
  snapshot() {
    this.#need();
    return new Uint8Array(this.mem.buffer, this.x.sc_region(), this.x.sc_region_bytes()).slice();
  }
  restore(snap) {
    this.#need();
    new Uint8Array(this.mem.buffer, this.x.sc_region(), snap.length).set(snap);
  }

  get sampleRate() { return this.x.sc_rate(); }

  #pcm(seconds) {
    const blocks = Math.ceil((seconds * this.sampleRate) / 128);
    const out = new Int16Array(blocks * 128);
    const max = this.x.sc_pcm_max() / 128;
    for (let done = 0; done < blocks;) {
      const n = this.x.sc_render(Math.min(max, blocks - done));
      out.set(new Int16Array(this.mem.buffer, this.x.sc_pcm(), n), done * 128);
      done += n / 128;
    }
    return out;
  }

  #refused() {
    const e = this.x.sc_play_error();
    const msg = e <= -100 ? EDIT_ERR[-100 - e] ?? `refused (${e})` : PLAY_ERR[e] ?? `refused (${e})`;
    return new CoreError(`no preview: ${msg}`, e);
  }

  // ---- Streaming (the editor's AudioWorklet): start something, then take blocks of it.
  // The song (a subtune from orderlist position `pos` of each voice).
  play(subtune = 0, pos = 0) {
    this.#need();
    if (!this.x.sc_play_from(subtune, pos)) throw this.#refused();
  }
  // Pattern p over and over on voice `channel` (0-2), the others muted; the song is untouched.
  playPattern(p, channel = 0) {
    this.#need();
    if (!this.x.sc_play_pattern(p, channel)) throw this.#refused();
  }
  // The song stopped, the instruments playable at notes (jam).
  audition() {
    this.#need();
    if (!this.x.sc_audition()) throw new CoreError("no preview: the audition did not start");
  }
  // Voice v plays note (0 C-0 .. 95) with instrument i; note null lets it go.
  jam(voice, note, i = 0) { return this.x.sc_jam(voice, note ?? 0xff, i) !== 0; }
  stop() { this.x.sc_stop(); }
  // The next `blocks` blocks of 128 samples through the effect slots: interleaved stereo.
  renderBlocks(blocks) {
    const n = this.x.sc_render_out(blocks);
    return new Int16Array(this.mem.buffer, this.x.sc_pcm_out(), n * 2).slice();
  }

  // `seconds` of whatever was started, through the effect slots: interleaved stereo PCM.
  renderOut(seconds) {
    const blocks = Math.ceil((seconds * this.sampleRate) / 128);
    const out = new Int16Array(blocks * 256);
    const max = this.x.sc_pcm_max() / 128;
    for (let done = 0; done < blocks;) {
      const k = Math.min(max, blocks - done);
      out.set(this.renderBlocks(k), done * 256);
      done += k;
    }
    return out;
  }

  // The song (a subtune from its start) through the FM-1's player and SID: mono 16-bit PCM at
  // sampleRate (the device's 44,118 Hz), `seconds` long: chip A, before the effect slots.
  renderSong({ subtune = 0, pos = 0, seconds = 10 } = {}) {
    this.play(subtune, pos);
    return this.#pcm(seconds);
  }

  // Pattern p on voice `channel` (mono, chip A).
  renderPattern(p, { channel = 0, seconds = 2 } = {}) {
    this.playPattern(p, channel);
    return this.#pcm(seconds);
  }

  // Instrument i (1-63) at a note (0 C-0 .. 95; 48 is C-4) on voice 1 with the song stopped,
  // held for `hold` seconds and released, `seconds` long in all.
  renderInstrument(i, { note = 48, hold = 0.5, seconds = 1.5 } = {}) {
    this.#need();
    if (!this.x.sc_audition()) throw new CoreError("no preview: the audition did not start");
    if (!this.x.sc_jam(0, note, i)) throw new CoreError("no preview: the note was refused");
    const held = this.#pcm(hold);
    this.x.sc_jam(0, 0xff, 0);
    const rest = this.#pcm(Math.max(0, seconds - hold));
    const out = new Int16Array(held.length + rest.length);
    out.set(held);
    out.set(rest, held.length);
    return out;
  }
}
