// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
const KEPT_KEY = "sidou-kept-sounds";
const KEPT_MAX = 16;
const BANK = { format: "sidou-bank/1", name: "KEPT" };

const stored = () => {
  try {
    const list = JSON.parse(localStorage.getItem(KEPT_KEY));
    return Array.isArray(list) ? list.filter((s) => typeof s?.name === "string" && typeof s?.text === "string") : [];
  } catch {
    return [];
  }
};
const store = (list) => {
  try {
    localStorage.setItem(KEPT_KEY, JSON.stringify(list));
  } catch {}
};

export function setupSounds({ $, el, button, say, actions, notify }) {
  const state = { kept: stored() };
  window.__sounds = state;
  const status = (text, kind = "") => say($("sid-sounds-state"), text, kind);

  const buttons = (sound, usable, ...more) => [
    ...(usable ? [button("Listen", () => actions.listen(sound.text))] : []),
    ...more,
    button("Edit as text", () => actions.edit(sound.text), { title: "Open it as a sidou-patch/1 patch in section 4" }),
    ...(usable ? [button("Send to the FM-1…", () => actions.send(sound.text), { disabled: true, className: "needs-fm1", title: "As a one-instrument song, the same as a patch" })] : []),
    button("Copy link", () => actions.link(sound.text), { title: "A #patch= link, the sound after the # that no server sees" }),
  ];

  const row = (sound, usable, ...more) => el("li", {},
    el("div", {}, el("b", { textContent: sound.name }), ` ${sound.about ?? ""}`),
    ...(sound.refused ? [el("div", { className: "simplified", textContent: `Not usable as it is: ${sound.refused}` })] : []),
    ...(sound.simplified?.length ? [el("div", { className: "simplified", textContent: `Simplified: ${sound.simplified.join("; ")}.` })] : []),
    el("div", { className: "acts" }, ...buttons(sound, usable, ...more)));

  const keep = (sound) => {
    if (state.kept.some((k) => k.text === sound.text)) return status(`${sound.name} is already kept.`);
    if (state.kept.length >= KEPT_MAX) return status(`${KEPT_MAX} sounds are kept, a bank's most: remove one first.`, "error");
    state.kept.push({ name: sound.name, about: sound.about, text: sound.text });
    store(state.kept);
    showKept();
    status(`${sound.name} kept (${state.kept.length} of ${KEPT_MAX}). It stays in this browser only.`, "ok");
  };

  const forget = (sound) => {
    state.kept = state.kept.filter((k) => k.text !== sound.text);
    store(state.kept);
    showKept();
  };

  const bankText = () => `${JSON.stringify({ ...BANK, patches: state.kept.map((k) => JSON.parse(k.text)) }, null, 2)}\n`;

  function showKept() {
    $("kept-list").replaceChildren(...state.kept.map((s) => row(s, true, button("Remove", () => forget(s)))));
    $("kept-panel").hidden = state.kept.length === 0;
    notify();
  }

  function showTune(sounds) {
    state.sounds = sounds;
    $("sid-sounds-list").replaceChildren(...sounds.map((s) => row(s, !s.refused, ...(s.refused ? [] : [button("Keep", () => keep(s))]))));
    $("sid-sounds").hidden = sounds.length === 0;
    status(sounds.length ? `${sounds.length} sound${sounds.length === 1 ? "" : "s"} found in the tune: listen, keep, edit or send them.` : "");
    notify();
  }

  $("kept-bank").addEventListener("click", () => actions.edit(bankText()));
  showKept();
  return { showTune, status };
}
