// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Tests for the keyboard controller (web/link-keyboard.js): the keymap from computer keyboard to
// PANEL frames and MIDI notes, focus handling (typing in inputs does not send), modifiers.
//   node web/link-keyboard.test.mjs
import assert from "node:assert/strict";
import { KEYMAP, KeyboardController } from "./link-keyboard.js";

let n = 0;
const test = (name, fn) => { fn(); n++; console.log(`ok - ${name}`); };

class FakeTarget {
  constructor(tag = 'DIV', isInput = false) {
    this.tagName = tag;
    this.isContentEditable = isInput;
  }
}

class FakeEvent {
  constructor(code, options = {}) {
    this.code = code;
    this.target = options.target || new FakeTarget();
    this.repeat = options.repeat || false;
    this._defaultPrevented = false;
  }
  preventDefault() { this._defaultPrevented = true; }
}

test("keymap covers piano keys on two rows", () => {
  const notes = KEYMAP.filter(m => m.note !== undefined);
  const lowerRow = ['KeyZ', 'KeyS', 'KeyX', 'KeyD', 'KeyC', 'KeyV', 'KeyG', 'KeyB', 'KeyH', 'KeyN', 'KeyJ', 'KeyM', 'Comma'];
  const upperRow = ['KeyQ', 'Digit2', 'KeyW', 'Digit3', 'KeyE', 'KeyR', 'Digit5', 'KeyT', 'Digit6', 'KeyY', 'Digit7', 'KeyU', 'KeyI'];

  for (const key of lowerRow) {
    assert.ok(notes.some(m => m.keys.includes(key)), `${key} in keymap`);
  }
  for (const key of upperRow) {
    assert.ok(notes.some(m => m.keys.includes(key)), `${key} in keymap`);
  }

  const c4 = notes.find(m => m.keys.includes('KeyZ'));
  assert.equal(c4.note, 60, "KeyZ is C4 (MIDI note 60)");

  const c5 = notes.find(m => m.keys.includes('KeyQ'));
  assert.equal(c5.note, 72, "KeyQ is C5 (MIDI note 72)");
});

test("keymap covers navigation and buttons", () => {
  const arrows = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
  for (const key of arrows) {
    assert.ok(KEYMAP.some(m => m.keys.includes(key)), `${key} in keymap`);
  }

  const selectUp = KEYMAP.find(m => m.keys.includes('ArrowUp'));
  assert.equal(selectUp.encoder, 0, "ArrowUp is SELECT encoder");
  assert.equal(selectUp.delta, 1, "ArrowUp turns clockwise");

  const buttons = [
    { key: 'Enter', button: 9, label: 'SAVE' },
    { key: 'Escape', button: 8, label: 'HOME' },
    { key: 'Space', button: 12, label: 'PLAY' },
    { key: 'Backspace', button: 10, label: 'ARP' },
  ];

  for (const { key, button } of buttons) {
    const m = KEYMAP.find(m => m.keys.includes(key));
    assert.ok(m, `${key} in keymap`);
    assert.equal(m.button, button, `${key} maps to button ${button}`);
  }
});

test("keyboard controller sends notes", () => {
  const notesPlayed = [];
  const notesStopped = [];

  const kb = new KeyboardController({
    client: null,
    playNote: (n) => notesPlayed.push(n),
    stopNote: (n) => notesStopped.push(n),
  });

  const down = new FakeEvent('KeyZ');
  kb.keydown(down);
  assert.ok(down._defaultPrevented, "KeyZ keydown prevents default");
  assert.deepEqual(notesPlayed, [60], "C4 played");

  const up = new FakeEvent('KeyZ');
  kb.keyup(up);
  assert.ok(up._defaultPrevented, "KeyZ keyup prevents default");
  assert.deepEqual(notesStopped, [60], "C4 stopped");
});

test("keyboard controller sends button presses", () => {
  const pressed = [];
  const released = [];

  const client = {
    press: (b) => pressed.push(b),
    release: (b) => released.push(b),
  };

  const kb = new KeyboardController({
    client,
    playNote: () => {},
    stopNote: () => {},
  });

  kb.keydown(new FakeEvent('Enter'));
  assert.deepEqual(pressed, [9], "SAVE pressed (button 9)");

  kb.keyup(new FakeEvent('Enter'));
  assert.deepEqual(released, [9], "SAVE released");
});

test("keyboard controller sends encoder turns", () => {
  const turns = [];

  const client = {
    press: () => {},
    release: () => {},
    turn: (e, d) => turns.push({ encoder: e, delta: d }),
  };

  const kb = new KeyboardController({
    client,
    playNote: () => {},
    stopNote: () => {},
  });

  kb.keydown(new FakeEvent('ArrowUp'));
  assert.deepEqual(turns, [{ encoder: 0, delta: 1 }], "SELECT turned up");

  kb.keydown(new FakeEvent('ArrowLeft'));
  assert.deepEqual(turns, [{ encoder: 0, delta: 1 }, { encoder: 1, delta: -1 }], "ALGORITHM turned left");
});

test("keyboard controller ignores keys in inputs", () => {
  const notesPlayed = [];
  const pressed = [];

  const client = { press: (b) => pressed.push(b) };
  const kb = new KeyboardController({
    client,
    playNote: (n) => notesPlayed.push(n),
    stopNote: () => {},
  });

  const inputEvent = new FakeEvent('KeyZ', { target: new FakeTarget('INPUT') });
  kb.keydown(inputEvent);
  assert.deepEqual(notesPlayed, [], "no note in INPUT");

  const selectEvent = new FakeEvent('Enter', { target: new FakeTarget('SELECT') });
  kb.keydown(selectEvent);
  assert.deepEqual(pressed, [], "no button in SELECT");

  const textareaEvent = new FakeEvent('Space', { target: new FakeTarget('TEXTAREA') });
  kb.keydown(textareaEvent);
  assert.deepEqual(pressed, [], "no button in TEXTAREA");

  const editableEvent = new FakeEvent('KeyZ', { target: new FakeTarget('DIV', true) });
  kb.keydown(editableEvent);
  assert.deepEqual(notesPlayed, [], "no note in contentEditable");
});

test("keyboard controller ignores repeats", () => {
  const notesPlayed = [];

  const kb = new KeyboardController({
    client: null,
    playNote: (n) => notesPlayed.push(n),
    stopNote: () => {},
  });

  kb.keydown(new FakeEvent('KeyZ'));
  assert.deepEqual(notesPlayed, [60], "first press plays");

  kb.keydown(new FakeEvent('KeyZ', { repeat: true }));
  assert.deepEqual(notesPlayed, [60], "repeat does not play again");
});

test("releaseAll stops all notes and releases all buttons", () => {
  const notesStopped = [];
  const released = [];

  const client = {
    press: () => {},
    release: (b) => released.push(b),
    turn: () => {},
  };

  const kb = new KeyboardController({
    client,
    playNote: () => {},
    stopNote: (n) => notesStopped.push(n),
  });

  kb.keydown(new FakeEvent('KeyZ'));
  kb.keydown(new FakeEvent('KeyX'));
  kb.keydown(new FakeEvent('Enter'));

  kb.releaseAll();

  assert.ok(notesStopped.includes(60), "C4 stopped");
  assert.ok(notesStopped.includes(62), "D stopped");
  assert.ok(released.includes(9), "SAVE released");
});


console.log(`1..${n}`);
console.log("# all keyboard tests passed");
test("multiple keys can be held at once", () => {
  const notesPlayed = [];
  const notesStopped = [];
  
  const kb = new KeyboardController({
    client: null,
    playNote: (n) => notesPlayed.push(n),
    stopNote: (n) => notesStopped.push(n),
  });
  
  kb.keydown(new FakeEvent('KeyZ'));
  kb.keydown(new FakeEvent('KeyX'));
  kb.keydown(new FakeEvent('KeyC'));
  
  assert.deepEqual(notesPlayed, [60, 62, 64], "chord played");
  
  kb.keyup(new FakeEvent('KeyX'));
  
  assert.deepEqual(notesStopped, [62], "D stopped, others still held");
  assert.equal(kb.notesOn.size, 2, "two notes still on");
});

