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
#define SONG_PAGES_LOOK 40u
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
void song_pages_table_meaning(uint32_t table, uint8_t left, uint8_t right, char out[31]);

extern const view song_pages_order_view;
extern const view song_pages_instr_view;
extern const view song_pages_table_view;

#ifdef __cplusplus
}
#endif

#endif
