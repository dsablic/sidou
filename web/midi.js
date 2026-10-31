// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { SongError, songToMidi } from "./songmidi.js";
import { Client, songsIn } from "./sysex.js";

const DEVICE_PORT = "SIDOU FM-1";

const SONGS = [
  { file: "chrome-comet.sng", name: "CHROME COMET", about: "Mid-eighties game music in D minor at 150 BPM. 80 s." },
  { file: "iron-rite.sng", name: "IRON RITE", about: "A heavy E Phrygian riff at 107 BPM. 85 s." },
  { file: "neon-highway.sng", name: "NEON HIGHWAY", about: "Synthwave in E minor at 125 BPM. 73 s." },
  { file: "paper-lanterns.sng", name: "PAPER LANTERNS", about: "A swung, chill D major track at 94 BPM. 82 s." },
];

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...kids) => {
  const e = Object.assign(document.createElement(tag), props);
  e.append(...kids);
  return e;
};

let current = null;
let url = null;

const stemOf = (name) => name.replace(/\.[^.]*$/, "").replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "song";

function show(bytes, stem, subtune = 0) {
  $("error").textContent = "";
  let r;
  try {
    r = songToMidi(bytes, { subtune });
  } catch (e) {
    $("result").hidden = true;
    $("error").textContent = e instanceof SongError ? e.message : `Could not convert this file: ${e.message}`;
    return;
  }
  current = { bytes, stem };
  const { song, walk, midi } = r;
  if (url) URL.revokeObjectURL(url);
  url = URL.createObjectURL(new Blob([midi.bytes], { type: "audio/midi" }));
  const multi = song.subtunes.length > 1;
  const file = `${stem}${multi ? `-${subtune + 1}` : ""}.mid`;
  const a = $("download");
  a.href = url;
  a.download = file;
  a.textContent = `Download ${file}`;
  $("r-name").textContent = song.name || stem;
  const counts = Array.from({ length: walk.channels }, (_, v) => walk.notes.filter((n) => n.voice === v).length);
  const mins = Math.floor(r.seconds / 60), secs = Math.round(r.seconds % 60);
  const rows = [
    ["Author", song.author || "-"],
    ["Length", `${mins}:${String(secs).padStart(2, "0")}, one pass`],
    ["Notes", `${counts.reduce((x, y) => x + y, 0)} (${counts.map((c, v) => `voice ${v + 1}: ${c}`).join(", ")})`],
    ["MIDI channels", walk.channels === 6 ? "1 to 6, one track a voice" : "1 to 3, one track a voice"],
    ["Tempo", `${Math.round(midi.bpm * 10) / 10} BPM, constant; ${midi.ppq} ticks a beat`],
    ["Speed commands", walk.tempo.length ? `${walk.tempo.length}, as markers in track 1` : "none"],
  ];
  $("r-info").replaceChildren(...rows.flatMap(([k, v]) => [el("dt", { textContent: k }), el("dd", { textContent: v })]));
  const sub = $("subtune");
  $("sub").hidden = !multi;
  if (multi && sub.options.length !== song.subtunes.length) {
    sub.replaceChildren(...song.subtunes.map((_, i) => el("option", { value: String(i), textContent: String(i + 1) })));
  }
  if (multi) sub.value = String(subtune);
  $("result").hidden = false;
  $("result").scrollIntoView({ block: "nearest" });
}

async function readFile(file) {
  if (!file) return;
  show(new Uint8Array(await file.arrayBuffer()), stemOf(file.name));
}

function buildCards() {
  for (const song of SONGS) {
    const button = el("button", { type: "button", textContent: "Make .mid" });
    button.addEventListener("click", async () => {
      try {
        const r = await fetch(`songs/${song.file}`);
        if (!r.ok) throw new Error(`could not load ${song.file} (HTTP ${r.status})`);
        show(new Uint8Array(await r.arrayBuffer()), stemOf(song.file));
      } catch (e) {
        $("error").textContent = e.message;
      }
    });
    $("songs").append(el("li", { className: "song" },
      el("h3", {}, song.name),
      el("p", { textContent: song.about }),
      el("div", { className: "act" }, button)));
  }
}

buildCards();
$("file").addEventListener("change", (e) => { readFile(e.target.files[0]); e.target.value = ""; });
$("subtune").addEventListener("change", (e) => { if (current) show(current.bytes, current.stem, Number(e.target.value)); });
const drop = $("drop");
for (const type of ["dragenter", "dragover"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.add("over"); });
for (const type of ["dragleave", "drop"]) drop.addEventListener(type, (e) => { e.preventDefault(); drop.classList.remove("over"); });
drop.addEventListener("drop", (e) => readFile(e.dataTransfer.files[0]));

const fm1 = { access: null, client: null, busy: false, files: [] };

function say(text, kind = "") {
  $("device").textContent = text;
  $("device").dataset.state = kind;
}

const version = (v) => `${v[0]}.${v[1]}.${v[2]}${v[3] ? ` (revision ${v[3]})` : ""}`;

function setBusy(busy) {
  fm1.busy = busy;
  $("connect").disabled = busy;
  $("check").disabled = busy || !fm1.access || !$("out").value || !$("in").value;
  for (const b of $("fm1").querySelectorAll("button")) b.disabled = busy || !fm1.client;
}

function fillPorts() {
  for (const [id, ports] of [["out", fm1.access.outputs], ["in", fm1.access.inputs]]) {
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
  fm1.client = null;
  fm1.files = [];
  $("fm1").hidden = true;
  $("fm1").replaceChildren();
  setBusy(false);
}

function listSongs() {
  const card = (title, about, target, name) => {
    const button = el("button", { type: "button", textContent: "Make .mid", disabled: !fm1.client });
    button.addEventListener("click", () => readSong(target, name, title, button));
    return el("li", { className: "song" }, el("h3", {}, title), el("p", { textContent: about }), el("div", { className: "act" }, button));
  };
  const cards = [card("The current song", "What the FM-1 has loaded and is editing now, with the changes you have not saved.", 2, "")];
  for (const f of songsIn(fm1.files)) cards.push(card(f.name, `A song stored on the FM-1, ${f.length} bytes.`, 0, f.name));
  if (cards.length === 1) cards.push(el("li", { className: "song" }, el("p", { textContent: "The FM-1's store has no other songs." })));
  $("fm1").replaceChildren(...cards);
  $("fm1").hidden = false;
}

async function readSong(target, name, title, button) {
  if (fm1.busy || !fm1.client) return;
  setBusy(true);
  $("error").textContent = "";
  const label = button.textContent;
  try {
    const bytes = await fm1.client.get({ target, name }, {
      onProgress: (d, t) => { button.textContent = t ? `Reading ${Math.round((100 * d) / t)}%` : label; },
    });
    show(bytes, stemOf(target === 2 ? "fm1-current-song" : name));
  } catch (e) {
    $("result").hidden = true;
    $("error").textContent = `Could not read ${title} from the FM-1: ${e.message}.`;
  } finally {
    button.textContent = label;
    setBusy(false);
  }
}

async function connect() {
  if (!navigator.requestMIDIAccess) return;
  setBusy(true);
  try {
    fm1.access = await navigator.requestMIDIAccess({ sysex: true });
  } catch (e) {
    say(`The browser refused MIDI access with SysEx (${e.message || e.name}). Allow it for this site and press Connect again.`, "error");
    setBusy(false);
    return;
  }
  fm1.access.onstatechange = () => { fillPorts(); forget(); say("A MIDI device changed: pick the ports and press Check this pair.", ""); };
  fillPorts();
  setBusy(false);
  const out = $("out").selectedOptions[0]?.textContent, inn = $("in").selectedOptions[0]?.textContent;
  if (out === DEVICE_PORT && inn === DEVICE_PORT) await check();
  else say(`No port named "${DEVICE_PORT}" found. Is the FM-1 plugged in and running SIDOU? Pick its output and input and press Check this pair; nothing is sent to other ports until you do.`, "error");
}

async function check() {
  const output = fm1.access.outputs.get($("out").value);
  const input = fm1.access.inputs.get($("in").value);
  if (!output || !input) return;
  forget();
  setBusy(true);
  for (const p of fm1.access.inputs.values()) p.onmidimessage = null;
  const client = new Client((bytes) => output.send(bytes));
  input.onmidimessage = (e) => client.push(e.data);
  say(`Asking ${output.name}…`);
  try {
    const info = await client.inquire();
    if (info.protocol !== 1) throw new Error(`it speaks SysEx protocol ${info.protocol}; this page speaks 1`);
    fm1.files = await client.dir();
    fm1.client = client;
    say(`SIDOU ${version(info.version)} on ${output.name}.`, "ok");
    listSongs();
  } catch (e) {
    say(e.message, "error");
  }
  setBusy(false);
}

if (!navigator.requestMIDIAccess) {
  $("support").hidden = false;
  $("connect").disabled = true;
  say("This browser has no Web MIDI.", "error");
} else {
  $("connect").addEventListener("click", connect);
  $("check").addEventListener("click", check);
  $("out").addEventListener("change", forget);
  $("in").addEventListener("change", forget);
}
