<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Contributing

- **Bugs** go in GitHub Issues. Please include the firmware version, say
  whether it happened on hardware or in the emulator, and attach a song file
  or key script that reproduces it.
- **Feature ideas** go in GitHub Discussions › Ideas. Popular ideas move to
  the public roadmap.
- **Pull requests:**
  - keep each PR to one change;
  - add tests, and run CI checks locally (`TESTING.md`, once it exists);
  - list any new third-party component in `LICENSING.md`;
  - add an SPDX header to every new file;
  - add an entry to `CHANGELOG.md`.
- **Licence of contributions:** by contributing, you license your work under
  GPL-3.0-or-later.
- **AI-assisted contributions** are welcome. You are responsible for them
  and must test them like any other change. Anything that affects the device
  must be tested in fm1-emu, and on hardware where possible.
