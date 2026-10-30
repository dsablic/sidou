// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The Songs page: Web MIDI (SysEx) to a connected FM-1, the songs from songs/*.sng, one PUT each
// through sysex.js. Nothing is sent before the Connect button, and only to the port pair selected.
import { Client, crc32 } from "./sysex.js";

const DEVICE_PORT = "SIDOU FM-1";
// name: the store file's name (a title, <= 16 characters in 20-5F); about: one line of its .md
const SONGS = [
  { file: "chrome-comet.sng", name: "CHROME COMET",
    about: "Mid-eighties game music in D minor at 150 BPM: bouncing pulse bass, a chirping lead, laser zaps and multiplexed drums. 80 s." },
  { file: "iron-rite.sng", name: "IRON RITE",
    about: "A heavy E Phrygian riff at 107 BPM with a snapping resonant filter, hard-sync growls, anvils and war toms. 85 s." },
  { file: "neon-highway.sng", name: "NEON HIGHWAY",
    about: "Synthwave in E minor at 125 BPM: a filter-swept saw bass, an arpeggio riff and a pulse-width modulated lead. 73 s." },
  { file: "paper-lanterns.sng", name: "PAPER LANTERNS",
    about: "A swung, chill D major track at 94 BPM: a triangle bass with a thump, a flute-like melody and breathing arpeggios. 82 s." },
];

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
};

const state = { access: null, client: null, info: null, files: [], busy: false, bytes: new Map(), cards: new Map() };

function say(node, text, kind = "") {
  node.textContent = text;
  node.dataset.state = kind;
}

const version = (v) => `${v[0]}.${v[1]}.${v[2]}${v[3] ? ` (revision ${v[3]})` : ""}`;

async function songBytes(song) {
  if (!state.bytes.has(song.file)) {
    const r = await fetch(`songs/${song.file}`);
    if (!r.ok) throw new Error(`could not load ${song.file} (HTTP ${r.status})`);
    state.bytes.set(song.file, new Uint8Array(await r.arrayBuffer()));
  }
  return state.bytes.get(song.file);
}

function buildCards() {
  const list = $("songs");
  for (const song of SONGS) {
    const button = el("button", { type: "button", textContent: "Send to FM-1", disabled: true });
    const progress = el("progress", { max: 1, value: 0, hidden: true });
    const status = el("div", { className: "state", role: "status" });
    const size = el("small");
    const dl = el("a", { href: `songs/${song.file}`, download: song.file, textContent: ".sng" });
    const card = el("li", { className: "song" },
      el("h3", {}, song.name, size, " ", dl),
      el("p", { textContent: song.about }),
      el("div", { className: "act" }, button),
      progress, status);
    card.dataset.song = song.name;
    button.addEventListener("click", () => run([song]));
    state.cards.set(song.name, { button, progress, status, size });
    list.append(card);
    songBytes(song).then((b) => { size.textContent = `${b.length} bytes`; }, (e) => say(status, e.message, "error"));
  }
}

function setBusy(busy) {
  state.busy = busy;
  const ready = !!state.info && !busy;
  $("send-all").disabled = !ready;
  $("check").disabled = busy || !state.access || !$("out").value || !$("in").value;
  $("connect").disabled = busy;
  for (const c of state.cards.values()) c.button.disabled = !ready;
}

function fillPorts() {
  const { access } = state;
  for (const [id, ports] of [["out", access.outputs], ["in", access.inputs]]) {
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
  state.info = null;
  state.client = null;
  $("files").textContent = "";
  for (const c of state.cards.values()) say(c.status, "");
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
  say($("device"), `Asking ${output.name}…`, "");
  try {
    const info = await client.inquire();
    if (info.protocol !== 1) throw new Error(`it speaks SysEx protocol ${info.protocol}; this page speaks 1`);
    state.files = await client.dir();
    state.client = client;
    state.info = info;
    say($("device"), `SIDOU ${version(info.version)} on ${output.name}: ${info.free} bytes free.`, "ok");
    showFiles();
  } catch (e) {
    say($("device"), `${e.message}`, "error");
  }
  setBusy(false);
}

function showFiles() {
  const songs = state.files.filter((f) => f.type === 1).map((f) => f.name);
  $("files").textContent = state.files.length
    ? `On the FM-1 now: ${state.files.map((f) => `${f.name} (${f.length} bytes)`).join(", ")}.`
    : "The FM-1's store has no files yet.";
  for (const song of SONGS) {
    const c = state.cards.get(song.name);
    if (songs.includes(song.name)) say(c.status, "Already on the FM-1.", "");
  }
}

// What a send of `songs` would overwrite: same name, different bytes.
async function plan(songs) {
  const todo = [], same = [], replace = [];
  for (const song of songs) {
    const bytes = await songBytes(song);
    const there = state.files.find((f) => f.name === song.name);
    if (there && there.length === bytes.length && there.crc === crc32(bytes)) same.push(song);
    else { todo.push(song); if (there) replace.push(song); }
  }
  return { todo, same, replace };
}

async function run(songs) {
  if (state.busy || !state.info) return;
  setBusy(true);
  const all = songs.length > 1;
  const note = $("all-state");
  note.textContent = "";
  try {
    const { todo, same, replace } = await plan(songs);
    for (const s of same) say(state.cards.get(s.name).status, "Already on the FM-1, identical: left alone.", "ok");
    if (replace.length && !window.confirm(`${replace.map((s) => s.name).join(", ")} ${replace.length > 1 ? "are" : "is"} already on the FM-1 with different content. Replace ${replace.length > 1 ? "them" : "it"}?`)) {
      for (const s of replace) say(state.cards.get(s.name).status, "Not replaced.", "");
      todo.splice(0, todo.length, ...todo.filter((s) => !replace.includes(s)));
    }
    let need = 0;
    for (const s of todo) need += (await songBytes(s)).length;
    if (need > state.info.free) say(note, `These need ${need} bytes and the FM-1 reports ${state.info.free} free: trying anyway, it will say if the store is full.`);
    let done = 0;
    for (const song of todo) {
      const c = state.cards.get(song.name);
      const bytes = await songBytes(song);
      c.progress.hidden = false;
      c.progress.value = 0;
      say(c.status, "Sending…");
      let waiting = false;
      try {
        await state.client.put(song.name, bytes, {
          onProgress: (d, t) => {
            if (waiting) { waiting = false; say(c.status, "Sending…"); }
            c.progress.value = t ? d / t : 1;
          },
          onBusy: (ms) => {
            waiting = true;
            say(c.status, `The FM-1 is tidying its store, waiting ${Math.round(ms / 1000)} s…`);
          },
          onWait: () => {
            waiting = true;
            say(c.status, "Waiting for the FM-1 to be quiet: stop the song playing on it (or the recording of its USB audio)…");
          },
        });
      } catch (e) {
        say(c.status, `Failed: ${e.message}.`, "error");
        if (all) say(note, `Stopped at ${song.name}.`, "error");
        return;
      } finally {
        c.progress.hidden = true;
      }
      say(c.status, "Sent. On the FM-1: menu, SONG, LOAD.", "ok");
      done++;
    }
    if (all) say(note, `${done} sent${same.length ? `, ${same.length} already there` : ""}.`, "ok");
    try {
      state.files = await state.client.dir();
      showFiles();
    } catch (e) {
      say($("device"), `Sent, but the FM-1's listing failed: ${e.message}.`, "error");
    }
  } catch (e) {
    say(note, e.message, "error");
  } finally {
    setBusy(false);
  }
}

buildCards();
if (!navigator.requestMIDIAccess) {
  $("support").hidden = false;
  $("connect").disabled = true;
  say($("device"), "This browser has no Web MIDI.", "error");
} else {
  $("connect").addEventListener("click", connect);
  $("check").addEventListener("click", check);
  $("send-all").addEventListener("click", () => run(SONGS));
  $("out").addEventListener("change", () => { forget(); });
  $("in").addEventListener("change", () => { forget(); });
}
