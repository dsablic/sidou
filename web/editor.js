// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The song editor page: a song from a file or the FM-1 (Web MIDI with SysEx: store.js and
// sysex.js, as the Library page), edited through SIDOU's own song code in wasm (song-edit.js over
// core.js), previewed by its own player (preview.js), saved back. Nothing is sent to MIDI before
// Connect, and only to the pair selected; nothing sounds before a press of Play or a key.
import { CoreError, EditError, loadCore } from "./core.js";
import { Preview } from "./preview.js";
import { FIRSTNOTE, INSTR_PARAMS, KEYOFF, KEYON, LASTNOTE, REST, SongSession, cellText, hex2, noteText,
  orderKind, orderText, parseHex, parseNote, parseOrder, tableMeaning } from "./song-edit.js";
import { Library, storeName } from "./store.js";
import { Client, getNakMessage } from "./sysex.js";

const DEVICE_PORT = "SIDOU FM-1";
const SAVED_MAX = 65536;

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
};
const say = (node, text, kind = "") => { node.textContent = text; node.dataset.state = kind; };
const kb = (n) => (n < 10240 ? `${n} bytes` : `${(n / 1024).toFixed(1)} KiB`);
const stemOf = (name) => name.replace(/\.[^.]*$/, "").replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "song";
const typing = (e) => e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement;

const state = {
  access: null, lib: null, client: null, port: "", busy: false, wasm: null, core: null, session: null, preview: null,
  tab: "instr", instr: 1, table: 0, tableAll: false, pattern: 0, row: 0, subtune: 0, octave: 4, voice: 0, saved: [],
};
window.__editor = state; // the smoke test (web/check.mjs) reads it

function save(bytes, filename) {
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
  const a = el("a", { href: url, download: filename });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  state.saved.push({ filename, bytes });
}

async function core() {
  if (!state.core) {
    state.wasm = await (await fetch("sidou-core.wasm")).arrayBuffer();
    state.core = await loadCore(state.wasm);
    state.session = new SongSession(state.core);
    state.preview = new Preview(state.wasm, { onMessage: previewNews });
  }
  return state.core;
}

// ---------------------------------------------------------------------------------------------
// The FM-1 (as the Library page connects)

function setBusy(busy) {
  state.busy = busy;
  const ready = !!state.lib && !busy;
  $("connect").disabled = busy;
  $("check").disabled = busy || !state.access || !$("out").value || !$("in").value;
  for (const b of document.querySelectorAll("#fm1-open button, #fm1-open select, .needs-fm1")) b.disabled = !ready;
  $("progress").hidden = !busy;
}

function fillPorts() {
  for (const [id, ports] of [["out", state.access.outputs], ["in", state.access.inputs]]) {
    const select = $(id);
    const previous = select.value;
    const names = [...ports.values()];
    select.replaceChildren(el("option", { value: "", textContent: "choose…" }),
      ...names.map((p) => el("option", { value: p.id, textContent: p.name })));
    const keep = names.find((p) => p.id === previous) ?? names.find((p) => p.name === DEVICE_PORT);
    select.value = keep ? keep.id : "";
    select.disabled = false;
  }
}

function forget() {
  state.lib = null;
  state.client = null;
  $("fm1-open").hidden = true;
  setBusy(false);
}

async function connect() {
  if (!navigator.requestMIDIAccess) return;
  setBusy(true);
  try {
    state.access = await navigator.requestMIDIAccess({ sysex: true });
  } catch (e) {
    say($("device"), `The browser refused MIDI access with SysEx (${e.message || e.name}). Allow it for this site and press Connect again.`, "error");
    setBusy(false);
    return;
  }
  state.access.onstatechange = () => { fillPorts(); forget(); say($("device"), "A MIDI device changed: pick the ports and press Check this pair.", ""); };
  fillPorts();
  setBusy(false);
  const out = $("out").selectedOptions[0]?.textContent, inn = $("in").selectedOptions[0]?.textContent;
  if (out === DEVICE_PORT && inn === DEVICE_PORT) await check();
  else say($("device"), `No port named "${DEVICE_PORT}" found. Is the FM-1 plugged in and running SIDOU? Pick its output and input and press Check this pair; nothing is sent to other ports until you do.`, "error");
}

async function check() {
  const output = state.access.outputs.get($("out").value);
  const input = state.access.inputs.get($("in").value);
  if (!output || !input) return;
  forget();
  setBusy(true);
  for (const p of state.access.inputs.values()) p.onmidimessage = null;
  const client = new Client((bytes) => output.send(bytes));
  input.onmidimessage = (e) => client.push(e.data);
  say($("device"), `Asking ${output.name}…`);
  try {
    const lib = new Library(client);
    await lib.refresh();
    state.lib = lib;
    state.client = client;
    state.port = output.name;
    showFm1();
  } catch (e) {
    say($("device"), e.message, "error");
  }
  setBusy(false);
}

function showFm1() {
  const { lib } = state;
  const songs = lib.files.filter((f) => f.type === 1);
  const v = lib.info.version;
  say($("device"), `SIDOU ${v[0]}.${v[1]}.${v[2]} on ${state.port}: ${songs.length} song${songs.length === 1 ? "" : "s"} saved, ${kb(lib.info.free)} free.`, "ok");
  $("stored").replaceChildren(...songs.map((f) => el("option", { value: f.name, textContent: f.name })));
  $("open-stored").hidden = !songs.length;
  $("stored").parentElement.hidden = !songs.length;
  $("fm1-open").hidden = false;
}

async function run(what, fn, note = $("save-state")) {
  if (state.busy || !state.lib) return;
  setBusy(true);
  say(note, `${what}…`);
  try {
    const done = await fn((d, t) => { $("progress").value = t ? d / t : 0; });
    say(note, done ?? "", done ? "ok" : "");
  } catch (e) {
    say(note, `${what} failed: ${e.message}.`, "error");
  } finally {
    setBusy(false);
  }
}

// ---------------------------------------------------------------------------------------------
// Opening a song

async function openSong(bytes, source) {
  $("song-error").textContent = "";
  await core();
  if (state.session.dirty && !window.confirm("The song open now has changes you have not saved or downloaded. Open the other one anyway?")) return;
  try {
    state.session.open(bytes, source);
  } catch (e) {
    $("song-error").textContent = e instanceof CoreError ? `The FM-1 would refuse this song: ${e.message}.` : `Could not open the song: ${e.message}.`;
    return;
  }
  state.preview.stop();
  const s = state.session.song;
  const first = s.orders[0]?.[0]?.entries.find((e) => e < 0xd0);
  Object.assign(state, { instr: 1, table: 0, tableAll: false, pattern: first ?? state.core.heldPatterns()[0] ?? 0, row: 0, subtune: 0 });
  $("save-name").value = source.fm1 && !source.current ? source.name : storeName(s.name || source.name || "SONG");
  $("editor").hidden = false;
  say($("status"), state.session.info.message ? `Opened, with a note from the reader: ${state.session.info.message}.` : "Opened.", "ok");
  say($("save-state"), "");
  changed();
}

async function pickFile(file) {
  if (file) await openSong(new Uint8Array(await file.arrayBuffer()), { name: file.name });
}

function openFromFm1(name) {
  return run(`Reading ${name}`, async (progress) => {
    const bytes = await state.lib.download(name, { onProgress: progress });
    await openSong(bytes, { name, fm1: true });
    return "";
  }, $("device"));
}

function openCurrent() {
  return run("Reading the current song", async (progress) => {
    let bytes;
    try {
      bytes = await state.client.get({ target: 2 }, { onProgress: progress });
    } catch (e) {
      throw new Error(getNakMessage(e));
    }
    await openSong(bytes, { name: "the FM-1's current song", fm1: true, current: true });
    return "";
  }, $("device"));
}

// ---------------------------------------------------------------------------------------------
// Editing: every change through the session, the page drawn again with the focus kept

function edit(label, fn) {
  try {
    state.session.apply(label, fn);
    say($("status"), label, "ok");
  } catch (e) {
    if (!(e instanceof EditError)) throw e;
    say($("status"), `${label}: ${e.message}`, "error");
  }
  changed();
}

function changed() {
  const key = document.activeElement?.dataset?.key;
  const s = state.session;
  state.preview.setSong(s.bytes());
  const used = state.core.savedBytes;
  $("size").textContent = `${used.toLocaleString("en")} of ${SAVED_MAX.toLocaleString("en")} bytes`;
  $("meter").firstElementChild.style.width = `${Math.min(100, (used / SAVED_MAX) * 100).toFixed(1)}%`;
  $("meter").classList.toggle("full", used > SAVED_MAX * 0.95);
  $("s-name").textContent = s.song.name || s.source.name;
  const from = !s.source.fm1 ? s.source.name : s.source.current ? "the FM-1's current song" : `the FM-1 (${s.source.name})`;
  $("s-from").textContent = `from ${from}${s.song.channels === 6 ? " · GoatTracker Stereo, 6 voices" : ""}${s.dirty ? " · changed" : ""}`;
  $("undo").disabled = !s.canUndo;
  $("redo").disabled = !s.canRedo;
  draw();
  if (key) {
    const again = document.querySelector(`[data-key="${CSS.escape(key)}"]`);
    if (again) { again.focus(); again.select?.(); }
  }
}

function draw() {
  for (const t of ["instr", "table", "pattern", "order"]) $(`p-${t}`).hidden = state.tab !== t;
  for (const b of document.querySelectorAll(".tabs button")) b.setAttribute("aria-selected", String(b.dataset.tab === state.tab));
  ({ instr: drawInstr, table: drawTable, pattern: drawPattern, order: drawOrder })[state.tab]();
  drawKeys();
}

// A cell: an input that shows `text`, checks what is typed with parse (null: refused) and makes
// the change with commit(value).
function cell(key, text, width, parse, commit, { readOnly = false, label = "" } = {}) {
  const input = el("input", { type: "text", className: `cell ${width}`, value: text, spellcheck: false, autocomplete: "off", readOnly });
  input.dataset.key = key;
  if (label) input.setAttribute("aria-label", label);
  input.addEventListener("change", () => {
    const v = parse(input.value);
    if (v === null) {
      input.classList.add("bad");
      say($("status"), `${label || "That"}: "${input.value}" is not a value here.`, "error");
      return;
    }
    input.classList.remove("bad");
    commit(v);
  });
  input.addEventListener("focus", () => input.select());
  return input;
}

// ---- Instruments (INSTR)

function drawInstr() {
  const c = state.core;
  const names = Array.from({ length: 63 }, (_, k) => {
    const b = c.instrument(k + 1);
    const name = String.fromCharCode(...b.slice(9)).replace(/\0[\s\S]*$/, "");
    return el("option", { value: String(k + 1), textContent: `${hex2(k + 1)} ${name || (b.some((x) => x) ? "(no name)" : "(free)")}` });
  });
  $("instr").replaceChildren(...names);
  $("instr").value = String(state.instr);
  const i = state.instr, b = c.instrument(i);
  const rows = INSTR_PARAMS.flatMap((p, f) => [
    el("label", { textContent: p, htmlFor: `ip${f}` }),
    Object.assign(cell(`i${f}`, hex2(b[f]), "w2", (t) => parseHex(t), (v) => edit(`Instrument ${hex2(i)} ${p}: ${hex2(v)}`, (k) => k.setInstrument(i, f, v)), { label: p }), { id: `ip${f}` }),
  ]);
  const name = String.fromCharCode(...b.slice(9)).replace(/\0[\s\S]*$/, "");
  rows.push(el("label", { textContent: "NAME", htmlFor: "ipname" }), Object.assign(cell("iname", name, "w16", (t) => (/^[\x20-\x7e]{0,16}$/.test(t) ? t : null), (t) => {
    edit(`Instrument ${hex2(i)} named ${t || "(none)"}`, (k) => {
      for (let j = 0; j < 16; j++) {
        const v = j < t.length ? t.charCodeAt(j) : 0;
        if (b[9 + j] !== v) k.setInstrument(i, 9 + j, v);
      }
    });
  }, { label: "Instrument name" }), { id: "ipname", maxLength: 16 }));
  $("params").replaceChildren(...rows);
}

// ---- Tables (TABLE)

function drawTable() {
  const t = state.table, tb = state.core.table(t);
  $("table").value = String(t);
  const shown = state.tableAll ? 255 : Math.min(255, Math.max(16, tb.rows + 8));
  $("table-all").hidden = shown === 255;
  const rows = [];
  for (let r = 0; r < shown; r++) {
    const side = (k) => cell(`t${t}.${r}.${k}`, hex2(k ? tb.right[r] : tb.left[r]), "w2", (x) => parseHex(x),
      (v) => edit(`${["WAVE", "PULSE", "FILTER", "SPEED"][t]} row ${hex2(r + 1)} ${k ? "right" : "left"}: ${hex2(v)}`, (c) => c.setTable(t, r, k, v)),
      { label: `Row ${hex2(r + 1)} ${k ? "right" : "left"}` });
    rows.push(el("tr", {}, el("td", { className: "n", textContent: hex2(r + 1) }), el("td", {}, side(0)), el("td", {}, side(1)),
      el("td", { className: "meaning", textContent: r < tb.rows || tb.left[r] || tb.right[r] ? tableMeaning(t, tb.left[r], tb.right[r]) : "" })));
  }
  $("rows").replaceChildren(...rows);
}

// ---- Patterns (a basic TRACKER grid)

function drawPattern() {
  const c = state.core, held = c.heldPatterns();
  if (!held.includes(state.pattern)) state.pattern = held[0] ?? 0;
  const named = new Set(state.session.song.orders.flat().flatMap((o) => o.entries.filter((e) => e < 0xd0)));
  $("pattern").replaceChildren(...held.map((p) => el("option", { value: String(p), textContent: `${hex2(p)} · ${c.pattern(p).length} rows${named.has(p) ? "" : " · not in an orderlist"}` })));
  $("pattern").value = String(state.pattern);
  const p = state.pattern, rows = c.pattern(p) ?? [];
  if (state.row >= rows.length) state.row = Math.max(0, rows.length - 1);
  $("length").value = String(rows.length);
  $("sel-row").textContent = rows.length ? `Row ${hex2(state.row)}:` : "";
  $("remove-pattern").disabled = named.has(p);
  $("remove-pattern").title = named.has(p) ? "An orderlist plays it: take it out of the orderlists first" : "";
  const write = (r, cellBytes, what) => edit(`Pattern ${hex2(p)} row ${hex2(r)}: ${what}`, (k) => k.setRow(p, r, cellBytes));
  const trs = rows.map((b, r) => {
    const note = cell(`p${p}.${r}.0`, noteText(b[0]), "w3", parseNote, (v) => write(r, [v, b[1], b[2], b[3]], noteText(v)), { label: `Row ${hex2(r)} note` });
    note.addEventListener("keydown", (e) => noteKey(e, p, r, b));
    const ins = cell(`p${p}.${r}.1`, b[1] ? hex2(b[1]) : "..", "w2", (t) => (/^\.+$|^$/.test(t.trim()) ? 0 : parseHex(t, 63)), (v) => write(r, [b[0], v, b[2], b[3]], `instrument ${hex2(v)}`), { label: `Row ${hex2(r)} instrument` });
    const cmd = cell(`p${p}.${r}.2`, b[2] ? hex2(b[2])[1] : ".", "w1", (t) => (/^\.?$/.test(t.trim()) ? 0 : parseHex(t, 15)), (v) => write(r, [b[0], b[1], v, v ? b[3] : 0], `command ${v.toString(16).toUpperCase()}`), { label: `Row ${hex2(r)} command` });
    const par = cell(`p${p}.${r}.3`, b[2] ? hex2(b[3]) : "..", "w2", (t) => (/^\.+$|^$/.test(t.trim()) ? 0 : parseHex(t)), (v) => write(r, [b[0], b[1], b[2], v], `parameter ${hex2(v)}`), { label: `Row ${hex2(r)} parameter` });
    const locks = c.locks(p, r).map((l) => `${l.name} ${l.value}`).join(", ");
    const tr = el("tr", { className: r === state.row ? "sel" : "" }, el("td", { className: "n", textContent: hex2(r), title: cellText(b) }),
      el("td", {}, note), el("td", {}, ins), el("td", {}, cmd), el("td", {}, par), el("td", { className: "locks", textContent: locks }));
    tr.addEventListener("focusin", () => selectRow(r));
    tr.firstChild.addEventListener("click", () => { selectRow(r); note.focus(); });
    return tr;
  });
  $("cells").replaceChildren(...trs);
}

function selectRow(r) {
  if (state.row === r) return;
  state.row = r;
  const trs = $("cells").children;
  for (let k = 0; k < trs.length; k++) trs[k].classList.toggle("sel", k === r);
  $("sel-row").textContent = `Row ${hex2(r)}:`;
}

// The computer keyboard as a piano: Z S X D C V G B H N J M , L . ; / from the octave's C, and Q 2
// W 3 E R 5 T 6 Y 7 U I 9 O 0 P an octave up (GoatTracker's layout).
const PIANO = {};
"zsxdcvgbhnjm,l.;/".split("").forEach((k, i) => { PIANO[k] = i; });
"q2w3er5t6y7ui9o0p".split("").forEach((k, i) => { PIANO[k] = 12 + i; });
const pianoNote = (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  const k = PIANO[e.key.toLowerCase()];
  if (k === undefined) return null;
  const n = state.octave * 12 + k;
  return n >= 0 && n <= LASTNOTE - FIRSTNOTE ? n : null;
};

function noteKey(e, p, r, b) {
  let v = null;
  const n = pianoNote(e);
  if (n !== null) v = FIRSTNOTE + n;
  else if (e.key === "Delete" || e.key === "Backspace") v = REST;
  else if (e.key === "=") v = KEYOFF;
  else if (e.key === "+") v = KEYON;
  else return;
  e.preventDefault();
  const instr = n !== null ? state.instr : b[1];
  edit(`Pattern ${hex2(p)} row ${hex2(r)}: ${noteText(v)}`, (k) => k.setRow(p, r, [v, instr, b[2], b[3]]));
  if (n !== null && !e.repeat) {
    sound(n);
    setTimeout(() => state.preview.release(state.voice), 250);
  }
  const next = document.querySelector(`[data-key="p${p}.${r + 1}.0"]`);
  if (next) next.focus();
}

// ---- Orderlists (ORDER)

function drawOrder() {
  const s = state.session.song, sub = Math.min(state.subtune, s.subtunes - 1);
  $("subtune").replaceChildren(...Array.from({ length: s.subtunes }, (_, k) => el("option", { value: String(k), textContent: String(k + 1) })));
  $("subtune").value = String(sub);
  const lists = s.orders[sub];
  const edits = Math.min(3, s.channels);
  $("order-note").textContent = s.channels === 6 ? "Voices 4-6 (a GoatTracker Stereo song's second chip) are shown and kept, not edited here." : "";
  $("o-head").replaceChildren(el("tr", {}, el("th", { textContent: "Pos" }), ...lists.map((_, ch) => el("th", { textContent: `Voice ${ch + 1}` }))));
  const longest = Math.max(...lists.map((o) => o.entries.length));
  const trs = [];
  for (let pos = 0; pos <= longest; pos++) {
    const tds = lists.map((o, ch) => {
      const ro = ch >= edits;
      if (pos < o.entries.length) {
        const e = o.entries[pos];
        const input = cell(`o${sub}.${ch}.${pos}`, orderText(e), "w3", parseOrder,
          (v) => edit(`Voice ${ch + 1} position ${hex2(pos)}: ${orderKind(v)}`, (k) => k.setOrder(sub, ch, pos, v)), { readOnly: ro, label: `Voice ${ch + 1} position ${hex2(pos)}` });
        input.title = orderKind(e);
        input.addEventListener("focus", () => { if (!ro) { $("o-voice").value = String(ch); $("o-pos").value = String(pos); } });
        return el("td", {}, input);
      }
      if (pos === o.entries.length) {
        const input = cell(`o${sub}.${ch}.rst`, hex2(o.restart), "w2", (t) => parseHex(t),
          (v) => edit(`Voice ${ch + 1} restarts at ${hex2(v)}`, (k) => k.setOrder(sub, ch, o.entries.length, v)), { readOnly: ro, label: `Voice ${ch + 1} restart position` });
        input.title = "RESTART AT: where the voice goes on after its last entry";
        input.addEventListener("focus", () => { if (!ro) { $("o-voice").value = String(ch); $("o-pos").value = String(pos); } });
        return el("td", {}, el("span", { className: "hint", textContent: "RST " }), input);
      }
      return el("td");
    });
    trs.push(el("tr", { className: "" }, el("td", { className: "n", textContent: hex2(pos) }), ...tds));
  }
  $("orders").replaceChildren(...trs);
  $("from").max = String(Math.max(0, longest - 1));
}

// ---------------------------------------------------------------------------------------------
// Preview

function previewNews(m) {
  if (m.type === "error") say($("pv-state"), m.message, "error");
  else if (m.type === "state") {
    const what = { idle: "", song: "Playing the song", pattern: "Playing the pattern", keys: "Keys", tail: "" }[m.state] ?? "";
    if (what || $("pv-state").dataset.state !== "error") say($("pv-state"), what && m.seconds ? `${what} · ${m.seconds.toFixed(1)} s` : what);
    state.playing = m.state;
  }
}

async function guard(fn) {
  try {
    await fn();
  } catch (e) {
    say($("pv-state"), e.message, "error");
  }
}

function sound(n) {
  state.voice = (state.voice + 1) % 3;
  guard(() => state.preview.note(state.voice, n, state.instr));
}

const held = new Map(); // a key or pointer: its voice
function keyDown(id, n) {
  if (held.has(id)) return;
  sound(n);
  held.set(id, state.voice);
  document.querySelector(`#keys [data-n="${n}"]`)?.classList.add("down");
}
function keyUp(id) {
  if (!held.has(id)) return;
  state.preview.release(held.get(id));
  held.delete(id);
  for (const b of document.querySelectorAll("#keys .down")) b.classList.remove("down");
}

function drawKeys() {
  $("octave").textContent = `Octave ${state.octave}`;
  const b = state.core.instrument(state.instr);
  const name = String.fromCharCode(...b.slice(9)).replace(/\0[\s\S]*$/, "");
  $("keys-instr").textContent = `The keys play instrument ${hex2(state.instr)}${name ? ` (${name})` : ""}.`;
  if ($("keys").dataset.octave === String(state.octave)) return;
  $("keys").dataset.octave = String(state.octave);
  const keys = [];
  for (let k = 0; k < 25; k++) {
    const n = state.octave * 12 + k;
    if (n > LASTNOTE - FIRSTNOTE) break;
    const black = [1, 3, 6, 8, 10].includes(k % 12);
    const key = el("button", { type: "button", className: black ? "black" : "", textContent: black ? "" : noteText(FIRSTNOTE + n), tabIndex: -1 });
    key.dataset.n = String(n);
    key.setAttribute("aria-label", noteText(FIRSTNOTE + n));
    key.addEventListener("pointerdown", (e) => { e.preventDefault(); key.setPointerCapture?.(e.pointerId); keyDown(`p${e.pointerId}`, n); });
    for (const t of ["pointerup", "pointercancel", "lostpointercapture"]) key.addEventListener(t, (e) => keyUp(`p${e.pointerId}`));
    keys.push(key);
  }
  $("keys").replaceChildren(...keys);
}

// ---------------------------------------------------------------------------------------------
// Saving

function saveName() {
  const name = $("save-name").value.trim().toUpperCase();
  $("save-name").value = name;
  return name;
}

function saveToFm1() {
  const name = saveName();
  if (!name) { say($("save-state"), "Give the song a name for the FM-1 first.", "error"); return; }
  return run(`Saving ${name} to the FM-1`, async (progress) => {
    const r = await state.session.saveToFm1(state.lib, name, {
      confirmReplace: async (n) => window.confirm(`${n} is already on the FM-1 with different content. Replace it with this song?`),
      onProgress: progress,
      onBusy: (ms) => say($("save-state"), `The FM-1 is tidying its store, waiting ${Math.round(ms / 1000)} s…`),
      onWait: () => say($("save-state"), "Waiting for the FM-1 to be quiet: stop the song playing on it…"),
    });
    showFm1();
    changed();
    if (r === "kept") return `${name} on the FM-1 was left as it is: give this song another name to keep both.`;
    if (r === "same") return `${name} on the FM-1 is already this song: nothing sent.`;
    return `Saved as ${name} on the FM-1 (${kb(state.core.savedBytes)}). Load it there from SONG, LOAD, or press Load now.`;
  });
}

function loadNow() {
  if (!window.confirm("Load this song on the FM-1 now, in place of the song it has loaded? (The FM-1 refuses if its song has changes it has not saved.)")) return;
  return run("Loading the song on the FM-1", async (progress) => {
    try {
      await state.session.loadNow(state.client, { onProgress: progress, onWait: () => say($("save-state"), "Waiting for the FM-1 to be quiet: stop the song playing on it…") });
    } catch (e) {
      if (e.nak === 4) throw new Error("the FM-1 is busy with its own song. If its screen says SAVE AS FIRST, its current song came from a computer and has no name yet: on the FM-1, SAVE AS (or load another song), then press Load now again. If the song there has changes it has not saved, save them or load another song first");
      if (e.nak) throw new Error(`the FM-1 did not load it (${e.message})`);
      throw e;
    }
    return "The FM-1 has loaded the song: it is its current song now (not saved there under a name until you SAVE AS).";
  });
}

function download(plain) {
  const s = state.session.song;
  const stem = stemOf(s.name || state.session.source.name || "song");
  if (!plain) { save(state.core.write().bytes, `${stem}.sng`); return; }
  const { bytes, warning } = state.core.write({ trailer: false });
  if (warning) {
    const other = /STEREO/.test(warning) ? "a 6-channel GoatTracker Stereo song" : "a 3-channel song";
    if (!window.confirm(`GoatTracker will read this plain file as ${other}, not as the ${s.channels}-channel song it is (${warning}). Download it anyway?`)) return;
  }
  save(bytes, `${stem}-goattracker.sng`);
}

// ---------------------------------------------------------------------------------------------

$("file").addEventListener("change", (e) => { pickFile(e.target.files[0]); e.target.value = ""; });
const drop = $("drop");
for (const type of ["dragenter", "dragover"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.add("over"); });
for (const type of ["dragleave", "drop"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.remove("over"); });
drop.addEventListener("drop", (e) => pickFile(e.dataTransfer.files[0]));

for (const b of document.querySelectorAll(".tabs button")) b.addEventListener("click", () => { state.tab = b.dataset.tab; draw(); });
$("undo").addEventListener("click", () => { const l = state.session.undo(); if (l) say($("status"), `Undone: ${l}`); changed(); });
$("redo").addEventListener("click", () => { const l = state.session.redo(); if (l) say($("status"), `Redone: ${l}`); changed(); });
$("instr").addEventListener("change", () => { state.instr = Number($("instr").value); draw(); });
$("gti5").addEventListener("click", () => {
  try {
    const b = state.core.instrument(state.instr);
    const name = String.fromCharCode(...b.slice(9)).replace(/\0[\s\S]*$/, "");
    save(state.core.exportInstrument(state.instr), `${stemOf(state.session.song.name || "song")}-${hex2(state.instr)}-${stemOf(name || "instrument")}.ins`);
  } catch (e) {
    say($("status"), e.message, "error");
  }
});
$("table").addEventListener("change", () => { state.table = Number($("table").value); state.tableAll = false; draw(); });
$("table-all").addEventListener("click", () => { state.tableAll = true; draw(); });
$("pattern").addEventListener("change", () => { state.pattern = Number($("pattern").value); state.row = 0; draw(); });
$("resize").addEventListener("click", () => {
  const n = Number($("length").value), p = state.pattern;
  edit(`Pattern ${hex2(p)}: ${n} rows`, (k) => k.resizePattern(p, n));
});
$("add-pattern").addEventListener("click", () => {
  const n = Number($("length").value) || 64;
  let made = null;
  edit(`New pattern of ${n} rows`, (k) => { made = k.addPattern(n); });
  if (made !== null) { state.pattern = made; state.row = 0; say($("status"), `New pattern ${hex2(made)} (${n} rows): put it in an orderlist to hear it in the song.`, "ok"); draw(); }
});
$("remove-pattern").addEventListener("click", () => { const p = state.pattern; edit(`Pattern ${hex2(p)} removed`, (k) => k.removePattern(p)); });
const rowOp = (what, fn) => () => { const p = state.pattern, r = state.row; edit(`Pattern ${hex2(p)} row ${hex2(r)}: ${what}`, (k) => fn(k, p, r)); };
$("push").addEventListener("click", rowOp("inserted (the length kept)", (k, p, r) => k.pushRow(p, r)));
$("pull").addEventListener("click", rowOp("deleted (the length kept)", (k, p, r) => k.pullRow(p, r)));
$("ins-row").addEventListener("click", rowOp("inserted", (k, p, r) => k.insertRow(p, r)));
$("del-row").addEventListener("click", rowOp("deleted", (k, p, r) => k.deleteRow(p, r)));
$("subtune").addEventListener("change", () => { state.subtune = Number($("subtune").value); draw(); });
$("o-insert").addEventListener("click", () => {
  const v = parseOrder($("o-value").value), ch = Number($("o-voice").value), pos = Number($("o-pos").value), sub = state.subtune;
  if (v === null) { say($("status"), `"${$("o-value").value}" is not an orderlist entry (04, R3, +2, -3).`, "error"); return; }
  edit(`Voice ${ch + 1}: ${orderKind(v)} inserted at ${hex2(pos)}`, (k) => k.insertOrder(sub, ch, pos, v));
});
$("o-delete").addEventListener("click", () => {
  const ch = Number($("o-voice").value), pos = Number($("o-pos").value), sub = state.subtune;
  edit(`Voice ${ch + 1}: position ${hex2(pos)} deleted`, (k) => k.deleteOrder(sub, ch, pos));
});

$("play").addEventListener("click", () => guard(() => state.preview.play(state.subtune, Number($("from").value) || 0)));
$("play-pattern").addEventListener("click", () => guard(() => state.preview.pattern(state.pattern, Number($("pv-voice").value))));
$("stop").addEventListener("click", () => state.preview.stop());
$("oct-down").addEventListener("click", () => { state.octave = Math.max(0, state.octave - 1); drawKeys(); });
$("oct-up").addEventListener("click", () => { state.octave = Math.min(7, state.octave + 1); drawKeys(); });
$("save-fm1").addEventListener("click", saveToFm1);
$("load-now").addEventListener("click", loadNow);
$("dl-sng").addEventListener("click", () => download(false));
$("dl-plain").addEventListener("click", () => download(true));

document.addEventListener("keydown", (e) => {
  if (!state.session?.core.loaded || $("editor").hidden) return;
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && !typing(e)) {
    e.preventDefault();
    $(e.shiftKey ? "redo" : "undo").click();
    return;
  }
  if (typing(e) || e.repeat) return;
  const n = pianoNote(e);
  if (n !== null) { e.preventDefault(); keyDown(`k${e.code}`, n); }
});
document.addEventListener("keyup", (e) => keyUp(`k${e.code}`));
addEventListener("blur", () => { for (const id of [...held.keys()]) keyUp(id); });
addEventListener("beforeunload", (e) => { if (state.session?.dirty) e.preventDefault(); });

if (!navigator.requestMIDIAccess) {
  $("support").hidden = false;
  $("connect").disabled = true;
  say($("device"), "This browser has no Web MIDI: the FM-1 needs Chrome or Edge. Songs from files work here.", "error");
} else {
  $("connect").addEventListener("click", connect);
  $("check").addEventListener("click", check);
  $("out").addEventListener("change", forget);
  $("in").addEventListener("change", forget);
  $("open-current").addEventListener("click", openCurrent);
  $("open-stored").addEventListener("click", () => { if ($("stored").value) openFromFm1($("stored").value); });
}
setBusy(false);

// A song handed over by the Library page's Edit (sessionStorage, this tab only).
try {
  const handed = JSON.parse(sessionStorage.getItem("sidou-edit") ?? "null");
  sessionStorage.removeItem("sidou-edit");
  if (handed?.hex) {
    const bytes = Uint8Array.from(handed.hex.match(/../g), (h) => parseInt(h, 16));
    openSong(bytes, handed.source ?? { name: "song" });
  }
} catch { /* no storage: nothing handed over */ }
