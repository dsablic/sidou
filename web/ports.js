// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import { Client } from "./sysex.js";

export const DEVICE_PORT = "SIDOU FM-1";
export const NO_FM1 = `No port named "${DEVICE_PORT}" found. Is the FM-1 plugged in and running SIDOU? Pick its output and input and press Check this pair; nothing is sent to other ports until you do.`;
export const CHANGED = "A MIDI device changed: pick the ports and press Check this pair.";
export const refusedWords = (e) => `The browser refused MIDI access with SysEx (${e.message || e.name}). Allow it for this site and press Connect again.`;

const option = (value, textContent) => Object.assign(document.createElement("option"), { value, textContent });

export function fillPorts(access, out, inn) {
  for (const [select, ports] of [[out, access.outputs], [inn, access.inputs]]) {
    const previous = select.value;
    const list = [...ports.values()];
    select.replaceChildren(option("", "choose…"), ...list.map((p) => option(p.id, p.name)));
    const keep = list.find((p) => p.id === previous) ?? list.find((p) => p.name === DEVICE_PORT);
    select.value = keep ? keep.id : "";
    select.disabled = false;
  }
}

export const pairIsTheFm1 = (out, inn) => [out, inn].every((s) => s.selectedOptions[0]?.textContent === DEVICE_PORT);

export function clientFor(access, out, inn) {
  const output = access.outputs.get(out.value);
  const input = access.inputs.get(inn.value);
  if (!output || !input) return null;
  for (const p of access.inputs.values()) p.onmidimessage = null;
  const client = new Client((bytes) => output.send(bytes));
  input.onmidimessage = (e) => client.push(e.data);
  return { client, port: output.name };
}
