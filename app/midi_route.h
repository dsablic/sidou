// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* MIDI's modes (M2e Task 3; design section 16, "Modes"): which channel plays what.
 *
 * - OMNI (the default, M1's): every channel plays the instrument (live), on the chip the keys
 *   play (app/keys_route.h: chip A with no song, chip B while one plays, chip A with KEYS A).
 * - LIVE: channel N alone plays the instrument; the others are ignored.
 * - MULTITIMBRAL: channels N, N+1, N+2 play chip A's voices 1, 2, 3, each a jam voice of the
 *   player with its own GoatTracker instrument (program change n is instrument n + 1), its
 *   own pitch bend (the range per channel, RPN 0) and its own notes: one at a time, the last
 *   one held sounding (a release goes back to the note held before it), with the LEGATO
 *   option a change of note while one is held glides on without a new attack. Channel N+3
 *   plays the instrument, on chip B.
 * - WHOLE-CHIP A: channel N plays all three voices of chip A with one GoatTracker instrument
 *   (channel N's program): three-voice polyphony, a new note taking the voice whose note
 *   started longest ago when all three sound. Options: CHORD (one key plays the stored chord:
 *   the note on voice 1, chord[0] and chord[1] semitones above it on voices 2 and 3, the last
 *   key held sounding) and ARP (the held notes cycled on voice 1 in the order they were
 *   pressed, a step every arp_rate player frames, or every 16th of the MIDI clock with
 *   arp_rate 0, so voices 2 and 3 stay the song's). Channel N+3 plays the instrument, on chip
 *   B.
 * - WHOLE-CHIP B: channel N plays the instrument on chip B (all three of its voices, as live
 *   does); chip A is left to the song.
 * In MULTITIMBRAL and both WHOLE-CHIP modes chip A belongs to the player, song or not:
 * keys_route_set_player tells keys_route, which treats it as a song playing, so the
 * instrument (the panel's keys, and the instrument's MIDI channel) moves to chip B with the
 * keys' setup, and chip A follows the song's setup (keys_route_song_wants).
 *
 * On chip A's channels (the jam channels):
 * - notes 12..107 (GoatTracker's C-0..B-7) play; others are ignored. Velocity is not used
 *   (GoatTracker's instruments have none);
 * - pitch bend: +-the channel's range (2 semitones, RPN 0 sets it per channel: CC 101/100 =
 *   0/0, then CC 6, or 96/97 to step), applied at the next midi_route_tick, coalesced;
 * - CC 74 (cutoff, 0..2047 by the map's curve) and CC 71 (resonance, 0..15) lock chip A's
 *   filter to the value, over the song's filter writes, until CC 121 or a change of mode;
 * - the chip-level CCs act on chip A's setup through keys_route_song_wants: 25 the model, 85
 *   the profile, 86, 87, 89, 90 the character's controls, 26 the envelope mode;
 * - CC 120 and 123 release the channel's voices; CC 121 also centres its bend and unlocks the
 *   filter;
 * - program change n (0..62) picks the channel's instrument n + 1; others are ignored;
 * - anything else is ignored: MIDI learn and the CC map act on the instrument's channel only.
 * The instrument's channel (OMNI: every channel) goes to live as in M1 (midi_rx_live), MIDI
 * learn's CCs and the map's included.
 *
 * The player's side is a small interface (midi_route_player), so this router stays portable
 * and host-tested; on the device song control binds it (core/player/song_ctl.h). Its
 * contract:
 * - jam_on(voice, note, instr, legato): voice (0..2) of chip A takes GoatTracker note (0
 *   C-0 .. 95 B-7) and instrument (1..63) at its next tick 0, as song_ctl_jam does, and from
 *   now on the voice is the jam's: the song's writes to it are held back (as M2c's mute does,
 *   with no gate-off) while the player's channel plays the jam note. legato 1: the note
 *   changes without a new attack (the gate stays on, as GoatTracker's tie). Each jam_on
 *   while the voice is the jam's replaces the note;
 * - jam_off(voice): the jam note's gate off (its release rings), and the voice goes back to
 *   the song at its next row (the song's registers again, as song_ctl_take's give-back);
 * - jam_bend(voice, offset): the voice's jam notes bent by offset (LIVE_PITCH_SEMITONE per
 *   semitone, signed) from now on, this note and the next, until the next jam_bend;
 * - filter(locks, cutoff, resonance): bit 0 of locks holds chip A's cutoff (0..2047) and bit
 *   1 its resonance (0..15) at the value, the song's writes to them held back; 0 gives them
 *   back to the song;
 * - each returns 1, or 0 when it cannot go now (its command ring is full): a note that cannot
 *   go is dropped (counted in midi_route.dropped), a bend or a filter is sent again at the
 *   next tick. A NULL function does nothing (returns 1); the device leaves them NULL until
 *   song_ctl has them (docs/plans/2026-10-15-m2e-midi.md, Task 3's results).
 *
 * Program change and bank select (M2e Task 4): CC 0 (MSB) and CC 32 (LSB) select a channel's
 * bank, MSB * 128 + LSB, held until changed (from either port; kept across lost input); a
 * program change means what the channel's bank says:
 * - bank 0, the channel's sound: on the instrument's channel the instrument's preset (M1's:
 *   0..LIVE_PRESET_COUNT - 1), on a chip A channel (MULTITIMBRAL, WHOLE-CHIP A) GoatTracker
 *   instrument n + 1;
 * - bank 1, songs: program n asks for the store's n-th song in name order, loaded stopped,
 *   on the instrument's channel or channel N only (midi_route_take_song: the song loader
 *   takes the request, M2c Task 8; a second one before it does replaces it);
 * - bank 2, the chip's profiles: 0..4 the built-in ones (SID_CHIP_*), on the instrument's
 *   channel live's profile (the keys' setup), on a chip A channel chip A's (its want path);
 *   5..12 USER 1..8 (midi_route_take_user_profile: M2a Task 6's user profiles take it);
 * - any other bank, or a program past a bank's range: nothing.
 * CC 0 and 32 are bank select on every channel the mode routes: MIDI learn leaves them
 * alone (midi_learn_learnable).
 *
 * The setup (midi_route_setup) is saved per song in its MIDI chunk (core/song/song_ext.h,
 * midi_route_setup_from_song, midi_route_setup_to_song); the global default stays in RAM
 * until the settings persist (M2a).
 *
 * Portable C with no hardware access. Main context. */
#ifndef SIDOU_MIDI_ROUTE_H
#define SIDOU_MIDI_ROUTE_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "midi_in.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The modes, as the MIDI chunk keeps them. */
typedef enum {
    MIDI_MODE_LIVE = 0,
    MIDI_MODE_MULTI = 1,
    MIDI_MODE_WHOLE_A = 2,
    MIDI_MODE_WHOLE_B = 3,
    MIDI_MODE_OMNI = 4,
    MIDI_MODE_COUNT = 5
} midi_mode;

/* WHOLE-CHIP A's options. */
typedef enum { MIDI_WHOLE_POLY = 0, MIDI_WHOLE_CHORD = 1, MIDI_WHOLE_ARP = 2 } midi_whole_option;

#define MIDI_ROUTE_VOICES 3u
/* Notes a jam channel keeps held, for the last-note priority and the arpeggio: the oldest
 * goes when a ninth comes. */
#define MIDI_ROUTE_HELD 8u
/* The instrument's channel in MULTITIMBRAL and WHOLE-CHIP A: N + 3. */
#define MIDI_ROUTE_LIVE_OFFSET 3u
#define MIDI_ROUTE_NO_NOTE 0xffu
/* GoatTracker's notes are MIDI's 12..107 (C-0 is MIDI 12). */
#define MIDI_ROUTE_GT_FIRST 12u
#define MIDI_ROUTE_GT_NOTES 96u
#define MIDI_ROUTE_INSTR_MAX 63u
/* MIDI clocks an ARP step with arp_rate 0: a 16th. */
#define MIDI_ROUTE_ARP_CLOCKS 6u
/* midi_route_tick's rate (live's), and the player's frames a tick (PAL's 50 Hz). */
#define MIDI_ROUTE_TICKS_PER_FRAME 2u
/* The defaults: the stored chord a major triad, the arpeggio a step every 3 frames, 4 rows a
 * beat. */
#define MIDI_ROUTE_CHORD_DEFAULT_2 4u
#define MIDI_ROUTE_CHORD_DEFAULT_3 7u
#define MIDI_ROUTE_ARP_DEFAULT 3u
#define MIDI_ROUTE_ROWS_DEFAULT 4u
#define MIDI_ROUTE_CHORD_MAX 24u
#define MIDI_ROUTE_ARP_MAX 32u
#define MIDI_ROUTE_ROWS_MAX 16u
/* Bank select (Task 4). */
#define MIDI_ROUTE_CC_BANK_MSB 0u
#define MIDI_ROUTE_CC_BANK_LSB 32u
enum { MIDI_ROUTE_BANK_SOUND = 0, MIDI_ROUTE_BANK_SONGS = 1, MIDI_ROUTE_BANK_PROFILES = 2 };
#define MIDI_ROUTE_BANKS 3u
#define MIDI_ROUTE_USER_PROFILES 8u
#define MIDI_ROUTE_NO_REQUEST 0xffu
/* Which chip a USER profile is asked for (midi_route_take_user_profile). */
enum { MIDI_ROUTE_CHIP_KEYS = 0, MIDI_ROUTE_CHIP_A = 1 };

typedef struct {
    uint8_t mode;          /* a midi_mode */
    uint8_t base;          /* N, 0..15 */
    uint8_t rows_per_beat; /* for the clock (M2e Task 2), 1..16 */
    uint8_t legato;        /* MULTITIMBRAL's and CHORD's option: 0 or 1 */
    uint8_t option;        /* a midi_whole_option */
    uint8_t chord[2];      /* semitones above the note on voices 2 and 3, 0..24 */
    uint8_t arp_rate;      /* frames an ARP step, 1..32; 0: a 16th of the clock */
} midi_route_setup;

typedef struct {
    int (*jam_on)(void* ctx, uint32_t voice, uint32_t note, uint32_t instr, uint32_t legato);
    int (*jam_off)(void* ctx, uint32_t voice);
    int (*jam_bend)(void* ctx, uint32_t voice, int32_t offset);
    int (*filter)(void* ctx, uint32_t locks, uint32_t cutoff, uint32_t resonance);
    void* ctx;
} midi_route_player;

/* A jam channel: chip A's voice 1, 2 or 3 in MULTITIMBRAL (WHOLE-CHIP A uses the first). */
typedef struct {
    uint8_t held[MIDI_ROUTE_HELD]; /* MIDI notes held, oldest first */
    uint8_t n;
    uint8_t instr;     /* 1..63 */
    uint16_t bend;     /* 0..16383 */
    uint8_t range;     /* semitones each way, 0..LIVE_BEND_RANGE_MAX */
    uint8_t rpn_msb, rpn_lsb; /* the RPN selected (127/127: none) */
} midi_route_chan;

typedef struct {
    uint8_t note;      /* the MIDI note it sounds (gate on), MIDI_ROUTE_NO_NOTE if none */
    uint32_t stamp;    /* when it started, for the stealing */
    int32_t bend_sent; /* the offset the player has */
} midi_route_voice;

typedef struct midi_route {
    midi_route_setup setup;
    midi_route_player player;
    live* l;
    keys_route* kr; /* NULL: no chip A setup from MIDI, nobody told who owns chip A */
    midi_route_chan ch[MIDI_ROUTE_VOICES];
    midi_route_voice v[MIDI_ROUTE_VOICES];
    /* Each channel's midi_route_to for its channel messages in this mode, so midi_rx sends the
     * instrument's straight on (midi_route_fast) */
    uint8_t to[16];
    /* Each channel's kinds (bit k: midi_kind k) that go straight to live: notes, aftertouch,
     * CCs and bend on the instrument's channels, none elsewhere (midi_route_fast). */
    uint8_t fast[16];
    /* Each channel's bank select, MSB and LSB: the router's channels write it here, the
     * instrument's through midi_map_cc (midi_map_port.banks, app/midi_rx.h). */
    uint8_t bank[16][2];
    uint8_t song_req;  /* the song a bank 1 program change asks for, or MIDI_ROUTE_NO_REQUEST */
    uint8_t user_req;  /* the USER profile (0..7) bank 2 asks for, or MIDI_ROUTE_NO_REQUEST */
    uint8_t user_chip; /* and for which chip (MIDI_ROUTE_CHIP_*) */
    uint8_t locks;      /* the filter's, as asked */
    uint8_t locks_sent; /* 0xff: to send */
    uint16_t cutoff;
    uint8_t resonance;
    uint8_t filter_dirty;
    uint8_t arp_pos;    /* the held note ARP sounds */
    uint8_t arp_wait;   /* ticks, or clocks with arp_rate 0, to the next step */
    uint32_t stamp;
    uint32_t dropped;   /* jam notes the player could not take */
} midi_route;

/* What a message is (midi_route_classify). */
typedef enum {
    MIDI_ROUTE_IGNORE = 0, /* nobody's: dropped */
    MIDI_ROUTE_LIVE = 1,   /* the instrument's: the caller maps it onto live (midi_rx_live) */
    MIDI_ROUTE_TAKE = 2    /* the router's: midi_route_take */
} midi_route_to;

/* The default setup: OMNI, N = channel 1, LEGATO off, POLY, a major triad, ARP every 3
 * frames, 4 rows a beat. */
void midi_route_setup_default(midi_route_setup* s);
/* The setup, the default; player may be NULL (none), kr may be NULL. */
void midi_route_init(midi_route* r, live* l, keys_route* kr, const midi_route_player* player);
/* A new setup (clamped): when anything changed, every jam voice is released, the filter
 * given back, the bends centred, live's notes released, and keys_route told whether the
 * player owns chip A. */
void midi_route_set(midi_route* r, const midi_route_setup* s);
/* 1 if chip A belongs to the player in this mode. */
int midi_route_owns_chip_a(uint32_t mode);
/* 1 if msg is surely the instrument's (a note, aftertouch, CC or bend on a channel to[] says
 * is live's: its bank select goes to bank[] through midi_map_cc; the program change and the
 * rest take the long way):
 * midi_rx's look before midi_route_classify, cheap for a stream of the instrument's CCs. */
#define midi_route_fast(r, msg) (((r)->fast[(msg)->channel & 15u] >> (msg)->kind) & 1u)
/* Where a message goes (pure). */
midi_route_to midi_route_classify(const midi_route* r, const midi_msg* msg);
/* 1 if the message, which classify gave the router, changes one of live's parameters (a
 * program change in bank 0 or 2 on the instrument's channel): MIDI out looks before it
 * (app/midi_rx.h). */
int midi_route_edits_live(const midi_route* r, const midi_msg* msg);
/* The song (0..127) a bank 1 program change asked for since the last call, or -1: the request
 * is cleared. */
int32_t midi_route_take_song(midi_route* r);
/* The USER profile (0..7) bank 2 asked for, and its chip (MIDI_ROUTE_CHIP_*), or -1. */
int32_t midi_route_take_user_profile(midi_route* r, uint32_t* chip);
/* A message classify gave the router: a jam channel's, or real time (the ARP's clock). */
void midi_route_take(midi_route* r, const midi_msg* msg);
/* Every jam note released and the jam channels' controllers reset (lost input,
 * app/midi_rx.h). */
void midi_route_reset_input(midi_route* r);
/* At LIVE_TICK_HZ: the bends and the filter that changed, and the ARP's steps by frames. */
void midi_route_tick(midi_route* r);

/* The setup a song's MIDI chunk holds (from the default for what it does not); 1 if it had
 * one. */
int midi_route_setup_from_song(const gt2_song* s, const midi_route_setup* dflt,
                               midi_route_setup* out);
/* The setup into the song's MIDI chunk: 0, or -1 if it does not fit. */
int midi_route_setup_to_song(gt2_song* s, const midi_route_setup* setup);
/* The setup as the chunk's fields, and back (clamped; an unknown mode is the default's). */
void midi_route_setup_encode(const midi_route_setup* s, song_midi* out);
void midi_route_setup_decode(const song_midi* in, const midi_route_setup* dflt,
                             midi_route_setup* out);

/* The modes' and options' names as the MIDI page shows them (at most 9 characters). */
const char* midi_route_mode_name(uint32_t mode);
const char* midi_route_option_name(uint32_t option);

#ifdef SIDOU_MIDI_MAP_DOC
/* For the generated reference (docs/reference/midi.md): what each mode does. */
const char* midi_route_mode_doc(uint32_t mode);
/* What a program change in each bank (0..MIDI_ROUTE_BANKS - 1) selects. */
const char* midi_route_bank_doc(uint32_t bank);
#endif

#ifdef __cplusplus
}
#endif

#endif
