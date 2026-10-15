// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// The build line on SIDOU's demo page: version.json, which tools/web-demo writes from git.
const el = document.getElementById("build");
try {
  const v = await (await fetch("version.json", { cache: "no-store" })).json();
  const commit = Object.assign(document.createElement("a"), {
    href: `https://github.com/dsablic/sidou/commit/${v.commit}`,
    textContent: v.short,
  });
  const emu = Object.assign(document.createElement("a"), {
    href: `https://github.com/joakimeriksson/fm1-emu/tree/${v.fm1_emu}`,
    textContent: v.fm1_emu.slice(0, 8),
  });
  const day = v.date.slice(0, 10);
  el.replaceChildren("Build ", commit, v.dirty ? " with local changes" : "", ` (${day}) · fm1-emu `, emu);
  el.title = `commit ${v.commit}, committed ${v.date}, built ${v.built}`;
} catch {
  el.textContent = "Build: unknown (no version.json)";
}
