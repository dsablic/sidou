<!-- SPDX-FileCopyrightText: 2026 SIDOU contributors -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Credits

SIDOU builds on, or learns from, the work of these people and projects.

## SID emulation
- **reSID**: Dag Lem, whose filter and DAC models live on in reSIDfp
- **reSIDfp**: Antti S. Lankila, Leandro Nini and contributors: SIDOU's
  reference, and the source of its tables (combined waveforms, filter cutoff
  and damping) and voice DACs

## Signal processing
- **State-variable filter**: Andrew Simper (Cytomic), whose
  "SvfLinearTrapOptimised2" paper defines the filter in SIDOU's SID core
- **Emilie Gillet (Mutable Instruments)**: the light reverb, ported from
  Rings/Clouds (MIT); the ensemble topology and the overdrive's gain law
  follow Plaits
- **Jon Dattorro**: the plate reverb ("Effect Design, Part 1"), and the
  chorus and flanger designs ("Effect Design, Part 2")
- **Julian Parker, Vadim Zavalishin, Efflam Le Bivic, Stefan Bilbao, Fabián
  Esqueda and Vesa Välimäki**: antiderivative anti-aliasing (ADAA), used by
  the tape, overdrive and fuzz
- **Udo Zölzer (ed.), *DAFX***, and **Vadim Zavalishin, *The Art of VA
  Filter Design***: the phaser and the fuzz's waveshaping
- **Jatin Chowdhury**: background on tape modelling

## FM-1 community
- **Felucca**: Leo Kuroshita / Hügelton Instruments, and the many forks
  built on it (SLOOP, Melodee, X0X, FoMni and others)
- **fm1-emu / fm1-sid**: Joakim Eriksson. fm1-emu (MIT) runs SIDOU's
  emulator tests; its web frontend (the wasm build, player, audio worklet
  and package unpacker) is the browser demo
- **fm1-custom-fw resource hub**: aroum and contributors
- **M-VAVE** for publishing the official open firmware

## Colours
- **The C64 palette**: SIDOU's 16 screen colours follow Philip "Pepto"
  Timmermann's measurements of the VIC-II colours

## Trackers and music
- **GoatTracker**: Lasse Öörni (Cadaver) and contributors: SIDOU's
  [song format](docs/decisions/0007-gt2-song-format.md)
  and its [player's reference](third_party/goattracker2/VENDORED.md)
- **Cadaver, Randall, Richard Bayliss, Hein, Jammer and No-XS**: GoatTracker's
  [example songs](LICENSING.md#music-and-content)
- **Klaus Dormann**: the [6502 functional test](third_party/6502-tests/VENDORED.md)
- **The 6502-rs developers**: the [`mos6502` crate](TESTING.md#song-player-reference)
- **SID-Wizard**: Mihály Horváth (Hermit)
- **High Voltage SID Collection (HVSC)** team

## Contributors
Contributors are credited here and in each release's changelog. That
includes people whose bug reports and feature ideas shaped SIDOU.
