// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//   node web/link-keyboard.test.mjs
import assert from "node:assert/strict";
import { KeyboardController } from "./link-keyboard.js";

let n = 0;
const test = (name, fn) => { fn(); n++; console.log(`ok - ${name}`); };

const SELECT = 0, ALGORITHM = 1;
const HOME = 8, SAVE = 9, ARP = 10, PLAY = 12, EDIT = 6;

const key = (code, { tag = "DIV", editable = false, repeat = false } = {}) => ({
  code, repeat, target: { tagName: tag, isContentEditable: editable }, preventDefault() {},
});

function rig() {
  const log = [];
  const client = {
    press: (b) => log.push(["press", b]),
    release: (b) => log.push(["release", b]),
    turn: (e, d) => log.push(["turn", e, d]),
    flush: () => log.push(["flush"]),
  };
  const kb = new KeyboardController({
    client,
    playNote: (note) => log.push(["on", note]),
    stopNote: (note) => log.push(["off", note]),
  });
  const tap = (code, opts) => { kb.keydown(key(code, opts)); kb.keyup(key(code, opts)); };
  return { log, kb, tap };
}

test("piano keys play and stop MIDI notes, two rows an octave apart", () => {
  const { log, tap } = rig();
  tap("KeyZ");
  tap("KeyQ");
  assert.deepEqual(log, [["on", 60], ["off", 60], ["on", 72], ["off", 72]]);
});

test("the buttons press and release the panel's", () => {
  const { log, tap } = rig();
  for (const code of ["Enter", "Escape", "Backspace", "Space"]) tap(code);
  assert.deepEqual(log, [SAVE, HOME, ARP, PLAY].flatMap((b) => [["press", b], ["release", b]]));
});

test("the arrows move TRACKER's cursor the way they point (SELECT clockwise is down)", () => {
  const { log, tap } = rig();
  tap("ArrowDown");
  tap("ArrowUp");
  tap("ArrowRight");
  assert.deepEqual(log, [["turn", SELECT, 1], ["turn", SELECT, -1], ["turn", ALGORITHM, 1]]);
});

test("Page Down is EDIT + SELECT, the turn sent while EDIT is held", () => {
  const { log, tap } = rig();
  tap("PageDown");
  assert.deepEqual(log, [["press", EDIT], ["turn", SELECT, 1], ["flush"], ["release", EDIT]]);
});

test("Shift is EDIT, and a page turn under it does not press EDIT twice", () => {
  const { log, kb } = rig();
  kb.keydown(key("ShiftLeft"));
  kb.keydown(key("PageUp"));
  kb.keyup(key("PageUp"));
  kb.keyup(key("ShiftLeft"));
  assert.deepEqual(log, [["press", EDIT], ["turn", SELECT, -1], ["release", EDIT]]);
});

test("nothing is sent while typing in a field", () => {
  const { log, tap } = rig();
  tap("KeyZ", { tag: "INPUT" });
  tap("Enter", { tag: "TEXTAREA" });
  tap("ArrowDown", { editable: true });
  assert.deepEqual(log, []);
});

test("a key's auto-repeat is ignored", () => {
  const { log, kb } = rig();
  kb.keydown(key("KeyZ"));
  kb.keydown(key("KeyZ", { repeat: true }));
  assert.deepEqual(log, [["on", 60]]);
});

test("losing focus lets every held note and button go", () => {
  const { log, kb } = rig();
  kb.keydown(key("KeyZ"));
  kb.keydown(key("ShiftLeft"));
  kb.blur();
  kb.keyup(key("KeyZ"));
  assert.deepEqual(log, [["on", 60], ["press", EDIT], ["off", 60], ["release", EDIT]]);
});

console.log(`link-keyboard.test.mjs: ${n} tests passed`);
