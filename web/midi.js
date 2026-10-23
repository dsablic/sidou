// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The Songs to MIDI page: a .sng (dropped, chosen, or one of the four showcase songs) becomes a Standard
// MIDI File with songmidi.js, offered as a download. No MIDI port is opened and nothing is sent anywhere.
import { SongError, songToMidi } from "./songmidi.js";

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

let current = null; // { bytes, stem }
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
