// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Keyboard control for the Live Link page: computer keyboard drives the FM-1's panel through PANEL
// frames and plays notes over MIDI. The device stays the source of truth. Designed from
// docs/reference/controls.md so each key maps to exactly what the device does.

export const KEYMAP = [
  { keys: ['KeyZ'], note: 60, label: 'C', black: false },
  { keys: ['KeyS'], note: 61, label: 'C#', black: true },
  { keys: ['KeyX'], note: 62, label: 'D', black: false },
  { keys: ['KeyD'], note: 63, label: 'D#', black: true },
  { keys: ['KeyC'], note: 64, label: 'E', black: false },
  { keys: ['KeyV'], note: 65, label: 'F', black: false },
  { keys: ['KeyG'], note: 66, label: 'F#', black: true },
  { keys: ['KeyB'], note: 67, label: 'G', black: false },
  { keys: ['KeyH'], note: 68, label: 'G#', black: true },
  { keys: ['KeyN'], note: 69, label: 'A', black: false },
  { keys: ['KeyJ'], note: 70, label: 'A#', black: true },
  { keys: ['KeyM'], note: 71, label: 'B', black: false },
  { keys: ['Comma'], note: 72, label: 'C', black: false },

  { keys: ['KeyQ'], note: 72, label: 'C', black: false },
  { keys: ['Digit2'], note: 73, label: 'C#', black: true },
  { keys: ['KeyW'], note: 74, label: 'D', black: false },
  { keys: ['Digit3'], note: 75, label: 'D#', black: true },
  { keys: ['KeyE'], note: 76, label: 'E', black: false },
  { keys: ['KeyR'], note: 77, label: 'F', black: false },
  { keys: ['Digit5'], note: 78, label: 'F#', black: true },
  { keys: ['KeyT'], note: 79, label: 'G', black: false },
  { keys: ['Digit6'], note: 80, label: 'G#', black: true },
  { keys: ['KeyY'], note: 81, label: 'A', black: false },
  { keys: ['Digit7'], note: 82, label: 'A#', black: true },
  { keys: ['KeyU'], note: 83, label: 'B', black: false },
  { keys: ['KeyI'], note: 84, label: 'C', black: false },

  { keys: ['Enter', 'NumpadEnter'], button: 9, label: 'SAVE', desc: 'Confirm/Save' },
  { keys: ['Escape'], button: 8, label: 'HOME', desc: 'Back/Menu' },
  { keys: ['Backspace'], button: 10, label: 'ARP', desc: 'Key off' },
  { keys: ['Space'], button: 12, label: 'PLAY', desc: 'Play/Stop' },

  { keys: ['ArrowUp'], encoder: 0, delta: 1, label: 'SELECT ↑', desc: 'Move cursor up' },
  { keys: ['ArrowDown'], encoder: 0, delta: -1, label: 'SELECT ↓', desc: 'Move cursor down' },
  { keys: ['ArrowLeft'], encoder: 1, delta: -1, label: 'ALGORITHM ←', desc: 'Navigate left' },
  { keys: ['ArrowRight'], encoder: 1, delta: 1, label: 'ALGORITHM →', desc: 'Navigate right' },

  { keys: ['PageUp'], encoder: 1, delta: 1, label: 'PageUp', desc: 'Page forward (ALGORITHM)' },
  { keys: ['PageDown'], encoder: 1, delta: -1, label: 'PageDown', desc: 'Page back (ALGORITHM)' },

  { keys: ['ShiftLeft', 'ShiftRight'], button: 6, label: 'EDIT', desc: 'Edit layer (hold)', modifier: true },
];

export class KeyboardController {
  constructor({ client, playNote, stopNote }) {
    this.getClient = typeof client === 'function' ? client : () => client;
    this.playNote = playNote;
    this.stopNote = stopNote;

    this.held = new Map();
    this.notesOn = new Set();
    this.modifiers = { edit: false };

    this.keydown = this.keydown.bind(this);
    this.keyup = this.keyup.bind(this);
    this.blur = this.blur.bind(this);
  }

  attach() {
    document.addEventListener('keydown', this.keydown);
    document.addEventListener('keyup', this.keyup);
    window.addEventListener('blur', this.blur);
  }

  detach() {
    document.removeEventListener('keydown', this.keydown);
    document.removeEventListener('keyup', this.keyup);
    window.removeEventListener('blur', this.blur);
    this.releaseAll();
  }

  shouldIgnore(e) {
    const tag = e.target.tagName;
    if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return true;
    if (e.target.isContentEditable) return true;
    return false;
  }

  keydown(e) {
    if (this.shouldIgnore(e)) return;
    if (e.repeat) return;

    const code = e.code;
    if (this.held.has(code)) return;

    for (const mapping of KEYMAP) {
      if (!mapping.keys.includes(code)) continue;

      if (mapping.modifier) {
        if (mapping.button === 6) this.modifiers.edit = true;
      } else if (mapping.note !== undefined) {
        e.preventDefault();
        this.held.set(code, { type: 'note', note: mapping.note });
        this.notesOn.add(mapping.note);
        this.playNote?.(mapping.note);
      } else if (mapping.button !== undefined) {
        e.preventDefault();
        this.held.set(code, { type: 'button', button: mapping.button });
        this.getClient()?.press(mapping.button);
      } else if (mapping.encoder !== undefined) {
        e.preventDefault();
        this.held.set(code, { type: 'encoder', encoder: mapping.encoder, delta: mapping.delta });
        this.getClient()?.turn(mapping.encoder, mapping.delta);
      }
      break;
    }
  }

  keyup(e) {
    if (this.shouldIgnore(e)) return;

    const code = e.code;
    const held = this.held.get(code);
    if (!held) return;

    for (const mapping of KEYMAP) {
      if (!mapping.keys.includes(code)) continue;

      if (mapping.modifier) {
        if (mapping.button === 6) this.modifiers.edit = false;
      } else if (held.type === 'note') {
        e.preventDefault();
        this.notesOn.delete(held.note);
        this.stopNote?.(held.note);
      } else if (held.type === 'button') {
        e.preventDefault();
        this.getClient()?.release(held.button);
      }
      break;
    }

    this.held.delete(code);
  }

  blur() {
    this.releaseAll();
  }

  releaseAll() {
    for (const note of this.notesOn) {
      this.stopNote?.(note);
    }
    this.notesOn.clear();

    for (const [code, held] of this.held) {
      if (held.type === 'button') {
        this.getClient()?.release(held.button);
      }
    }
    this.held.clear();
    this.modifiers.edit = false;
  }
}
