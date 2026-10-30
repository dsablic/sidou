// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI in from the TRS jack and USB onto the live instrument, and MIDI out over USB.
 *
 * The TIMER5 interrupt polls Felucca's USB stack and TRS UART driver (usb/, vendored), each
 * at 2 kHz. They push every USB-MIDI packet and every TRS byte, stamped with fm1_ticks() at
 * the poll, onto a ring (app/midi_ring.h). The main loop drains the ring through a parser per
 * source onto live (app/midi_rx.h, which has the default mapping). */
#ifndef SIDOU_FM1_MIDI_H
#define SIDOU_FM1_MIDI_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "midi_learn.h"
#include "midi_route.h"
#include "midi_sync.h"
#include "sysex.h"

/* Interrupts off, before TIMER5 starts: the UART (a port H read-modify-write) and USB (which
 * waits 25 ms for the controller's reset). l receives the messages, through MIDI's mode
 * (app/midi_route.h), which tells kr when chip A is the player's. */
void midi_init(live* l, keys_route* kr);
/* MIDI's mode, which the MIDI page edits (platform/fm1/ui.c). In RAM, OMNI at power-on, until
 * the settings persist (M2a). Its player side is unbound until song control has jam release,
 * take-over and bend (https://github.com/dsablic/sidou/blob/v0.5.0/docs/plans/2026-10-15-m2e-midi.md, Task 3's results): chip A's channels
 * are routed and tracked, and play nothing yet. */
midi_route* midi_routed(void);
/* Main loop, once per live tick, after live_tick: the mode's bends, filter and ARP steps. */
void midi_tick(void);
/* MIDI clock and transport (app/midi_sync.h): main.c binds its player over song control.
 * Its settings (CLOCK, CLOCK OUT) are in RAM until the settings persist (M2a). */
midi_sync* midi_synced(void);
/* The clock's requests to the player (start, stop, continue, seeks, the period and the
 * alignment) and clock out's packets over USB, sent at once (the TIMER5 interrupt's USB poll
 * takes them), each before a commit: from midi_tick (100 Hz), from midi_drain after a clock or
 * a transport message, and from main.c's play() while CLOCK OUT is on (about once a
 * millisecond, with the key scan: clock out's times). */
void midi_sync_pass(void);
/* Main loop, right after an operation with interrupts masked (store_fm1.c: a flash erase or
 * program), from `from` to `to` (fm1_ticks()): the clocks the polls stamped late for it stay
 * out of the clock's fit (midi_clock_hold), before midi_drain takes them. */
void midi_clock_held(uint32_t from, uint32_t to);
/* MIDI learn's bindings (app/midi_learn.h), which both inputs and MIDI out use: the screen
 * arms learn on them (platform/fm1/ui.c). In RAM, empty at power-on. */
midi_learn* midi_learned(void);
/* TIMER5 interrupt, each at 2 kHz, on different ticks. */
void midi_usb_isr(void);
void midi_uart_isr(void);
/* Main loop: takes what came in onto live, leaving the register ring room for the rest of
 * the pass (reg_ring.h's worst pass), and recovers from lost input (app/midi_rx.h). Returns
 * 1 if it may have written registers: the caller then commits them (audio_commit). */
int midi_drain(void);
/* Main loop, once per live tick (100 Hz): the panel's parameter changes out over USB as
 * CCs, NRPNs and program changes (app/midi_out.h), into the IN endpoint's ring (src/usb.c),
 * which the TIMER5 interrupt sends. Never waits: with no room, a change stays pending (its
 * latest value goes later); with no host, it is dropped. */
void midi_send(void);
/* Main loop: a USB retry while no host answers (once a second). */
void midi_service(uint32_t now_ms);
/* 1 while a host has the device configured and is awake (its frame number moves): the main
 * loop then never sleeps in `idle` (main.c wait_for_interrupt). */
int midi_usb_live(void);
/* Non-real-time activity (midi_rx.inputs): completed messages and every SysEx byte,
 * for the store's quiet detector. Clock and sensing do not disturb it. */
uint32_t midi_inputs(void);
/* Main-loop binding and bounded service for SIDOU's independent SysEx stream. */
void midi_sysex_bind(const sysex_hooks* hooks, const volatile uint32_t* now_ms);
/* `loud`: the store's held programs wait for quiet now (store_fm1.c), so a write's ACK waits
 * and the sender hears WAIT (app/sysex.h sysex.loud). */
void midi_sysex_service(uint32_t now_ms, int loud);
/* 1 while a computer records the USB audio (it has the stream open and takes its packets). */
int midi_usb_recording(void);
extern uint8_t midi_sysex_running;
extern uint8_t midi_sysex_writing;

#if defined(SIDOU_FLASH_SPIKE)
/* The flash spike only (flash_spike.c), which plays nothing: empties the input ring, and gives
 * the TRS bytes taken since boot and those lost (the UART's 128-byte DMA ring overwritten
 * before a poll, or the input ring full; it is emptied often enough that only the first can
 * happen while the spike measures). */
void midi_spike_trs(uint32_t* bytes, uint32_t* lost);
#endif

#endif
