// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SONG_PAGES_H
#define SIDOU_SONG_PAGES_H

#include "song_view.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SONG_PAGES_ROWS 12u
#define SONG_PAGES_TOP 3u
/* ORDER: the other two voices' entries at 2x (4 cells each), and their heads. */
#define SONG_PAGES_OTHER_COL 18u
#define SONG_PAGES_OTHER_STEP 6u
#define SONG_PAGES_BUILD_ROWS 3u
#define SONG_PAGES_LOOK 56u
#define SONG_PAGES_ENC_K2 (PANEL_ENC_K1 + 1u)
#define SONG_PAGES_ENC_PRESETS 6u
/* Prospective edit_guard requests: pattern = subtune/instrument/table, row = entry/field/
 * row, cell[0] = value, cell[1] = voice (ORDER) or side (TABLE). RST uses row == entries.
 * Distinct from Task 3's pattern-row requests; storage guards must handle these kinds. */
enum {
    SONG_PAGES_EDIT_ORDER = 0x100, SONG_PAGES_EDIT_INSERT, SONG_PAGES_EDIT_DELETE,
    SONG_PAGES_EDIT_INSTR, SONG_PAGES_EDIT_TABLE, SONG_PAGES_EDIT_RESTART
};
enum {
    SONG_PAGES_READY, SONG_PAGES_WAITING, SONG_PAGES_TIDYING, SONG_PAGES_SAVING,
    SONG_PAGES_FULL, SONG_PAGES_INVALID, SONG_PAGES_UNAVAILABLE, SONG_PAGES_BUSY
};

/* The ARP helper on TABLE's WAVE table (docs/reference/controls.md "ARP helper"): an arpeggio
 * as semitone offsets over the played note, written as plain GT2 wavetable rows. */
#define SONG_PAGES_ARP_STEPS 8u
/* The rows an arpeggio takes at most: its steps, the row that loops step 1 back and the jump. */
#define SONG_PAGES_ARP_ROWS (SONG_PAGES_ARP_STEPS + 2u)
#define SONG_PAGES_ARP_SPEED_MAX 16u
#define SONG_PAGES_ARP_WAVE 0x41u
typedef struct {
    int8_t off[SONG_PAGES_ARP_STEPS]; /* -64..63 semitones over the note played */
    uint8_t steps;                    /* 1..SONG_PAGES_ARP_STEPS */
    uint8_t speed;                    /* ticks a step lasts, 1..SONG_PAGES_ARP_SPEED_MAX */
    uint8_t loop;                     /* the step the arpeggio returns to, 1..steps; 0 holds the last */
    uint8_t wave;                     /* the waveform byte, 10..DF */
} song_pages_arp;

typedef struct {
    song_view* sv;
    /* Retained while an edit waits, even if the user leaves the page. */
    const song_transport* transport;
    song_edit_request request;
    gt2_edit edit;
    uint32_t ticket, revision;
    uint32_t unbuilt;
    uint16_t order_cursor; /* 0: subtune selector, 1..entries: entries, entries+1: RST */
    uint8_t subtune, instr_cursor, table, table_row;
    uint8_t page; /* last callback's identity, not the active view or canvas owner */
    uint8_t pending, prepared, sent, auxiliary, status;
    gt2_err error;
    /* The ARP helper: open, the step the cursor is on, the table row it starts at (0 based), the
     * rows it read, the next byte to write and where they end; the draft and the bytes to write
     * (left, right of each row). */
    uint8_t arp, arp_cur, arp_start, arp_len, arp_next, arp_end;
    /* The draft as the player hears it (song_transport.draft): on its voice, and a change to
     * send (published on open and every change, taken off once closed and its rows written). */
    uint8_t arp_heard, arp_voice, arp_dirty;
    song_pages_arp arp_draft;
    uint8_t arp_rows[2u * SONG_PAGES_ARP_ROWS];
    uint8_t drawn[SONG_PAGES_LOOK];
} song_pages;

/* All three view states are song_pages*, never song_view*. No second canvas is allocated. */
void song_pages_init(song_pages* pages, song_view* sv);
/* Once per main-loop pass, also outside song views. One preparation/compaction move at most.
 * Finish pending work before rebinding or replacing the transport's song. */
int song_pages_poll(song_pages* pages);
const char* song_pages_message(const song_pages* pages);
/* Popup text for an encoder of the actual showing view, independent of the shared build's
 * page. Matching pending bytes are previewed without polling. Zero max disables the bar. */
int song_pages_popup(const song_pages* pages, const view* owner, uint32_t encoder, int edit,
                     char name[SCREEN_POPUP_TEXT], char value[SCREEN_POPUP_TEXT],
                     uint32_t* fill, uint32_t* max);
/* Global SEQ routing must try this before its normal cycle (only INSTR + EDIT + SEQ).
 * 1: jumped to TABLE, 0: not that gesture. */
int song_pages_jump(song_pages* pages, screen* s, const screen_event* event);
/* `a` in plain GT2 wavetable rows from table row `first` (1 based, as pointers say it): left[] and
 * right[] (SONG_PAGES_ARP_ROWS each), the rows count back. Row 1 is the waveform and the first
 * offset; the others a delay of speed - 1 ticks (speed 1: the waveform again) and their offset; a
 * jump (FF) to the loop step, or FF 00 to hold the last note. Looping back to step 1 over
 * delay rows takes one more row (a delay keeps the note before it its time). */
uint32_t song_pages_arp_encode(const song_pages_arp* a, uint32_t first,
                               uint8_t left[SONG_PAGES_ARP_ROWS], uint8_t right[SONG_PAGES_ARP_ROWS]);
/* The arpeggio the `avail` rows from `first` hold, if they are exactly such rows (either of
 * encode's forms, or the hand-made one with the waveform in every row): 1 and its rows into *rows;
 * 0 when they are anything else. */
int song_pages_arp_decode(const uint8_t* left, const uint8_t* right, uint32_t first, uint32_t avail,
                          song_pages_arp* a, uint32_t* rows);
/* A default arpeggio: a major chord over the note, 2 ticks a step, looping at the first. */
void song_pages_arp_default(song_pages_arp* a);
/* The ARP button on the WAVE table's cursor row: opens the helper on the arpeggio there, or a new
 * one on a blank row. 1 when it opened, else a toast says why. */
int song_pages_arp_open(song_pages* pages, screen* s);
/* SAVE in the helper: writes the rows (through the edit queue, a cell at a time) and closes it. */
void song_pages_arp_save(song_pages* pages, screen* s);
/* HOME in the helper closes it without writing (and the draft goes from the player); 1 if it
 * was open. While it is open the keys play the draft (song_transport.draft): song_pages_poll
 * sends it, and each change, and takes it off once the helper has closed and its rows are
 * written; poll while arp_dirty as while pending. */
int song_pages_arp_close(song_pages* pages);

void song_pages_table_meaning(uint32_t table, uint8_t left, uint8_t right, char out[31]);

extern const view song_pages_order_view;
extern const view song_pages_instr_view;
extern const view song_pages_table_view;

#ifdef __cplusplus
}
#endif

#endif
