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
#define SONG_PAGES_OTHER_COL 18u
#define SONG_PAGES_OTHER_STEP 6u
#define SONG_PAGES_BUILD_ROWS 3u
#define SONG_PAGES_LOOK 56u
#define SONG_PAGES_ENC_K2 (PANEL_ENC_K1 + 1u)
#define SONG_PAGES_ENC_PRESETS 6u
enum {
    SONG_PAGES_EDIT_ORDER = 0x100, SONG_PAGES_EDIT_INSERT, SONG_PAGES_EDIT_DELETE,
    SONG_PAGES_EDIT_INSTR, SONG_PAGES_EDIT_TABLE, SONG_PAGES_EDIT_RESTART
};
enum {
    SONG_PAGES_READY, SONG_PAGES_WAITING, SONG_PAGES_TIDYING, SONG_PAGES_SAVING,
    SONG_PAGES_FULL, SONG_PAGES_INVALID, SONG_PAGES_UNAVAILABLE, SONG_PAGES_BUSY
};

#define SONG_PAGES_ARP_STEPS 8u
#define SONG_PAGES_ARP_ROWS (SONG_PAGES_ARP_STEPS + 2u)
#define SONG_PAGES_ARP_SPEED_MAX 16u
#define SONG_PAGES_ARP_WAVE 0x41u
typedef struct {
    int8_t off[SONG_PAGES_ARP_STEPS];
    uint8_t steps;
    uint8_t speed;
    uint8_t loop;
    uint8_t wave;
} song_pages_arp;

#define SONG_PAGES_BATCH_BYTES 80u
#define SONG_PAGES_BATCH_RUNS 8u
#define SONG_PAGES_RUN_INSTR GT2_TABLES
#define SONG_PAGES_RUN_BLANK 0xffu
typedef struct {
    uint8_t kind, index, n, at;
} song_pages_run;

typedef struct song_pages {
    song_view* sv;
    const song_transport* transport;
    song_edit_request request;
    gt2_edit edit;
    uint32_t ticket, revision;
    uint32_t unbuilt;
    uint16_t order_cursor;
    uint8_t subtune, instr_cursor, table, table_row;
    uint8_t page;
    uint8_t pending, prepared, sent, auxiliary, status;
    gt2_err error;
    uint8_t arp, arp_cur, arp_start, arp_len;
    uint8_t arp_heard, arp_voice, arp_dirty;
    song_pages_arp arp_draft;
    uint8_t batch_next, batch_end, batch_runs, batch_used;
    song_pages_run batch_run[SONG_PAGES_BATCH_RUNS];
    uint8_t batch_bytes[SONG_PAGES_BATCH_BYTES];
    uint8_t drawn[SONG_PAGES_LOOK];
} song_pages;

void song_pages_init(song_pages* pages, song_view* sv);
int song_pages_poll(song_pages* pages);
const char* song_pages_message(const song_pages* pages);
int song_pages_popup(const song_pages* pages, const view* owner, uint32_t encoder, int edit,
                     char name[SCREEN_POPUP_TEXT], char value[SCREEN_POPUP_TEXT],
                     uint32_t* fill, uint32_t* max);
int song_pages_jump(song_pages* pages, screen* s, const screen_event* event);
uint32_t song_pages_arp_encode(const song_pages_arp* a, uint32_t first,
                               uint8_t left[SONG_PAGES_ARP_ROWS], uint8_t right[SONG_PAGES_ARP_ROWS]);
int song_pages_arp_decode(const uint8_t* left, const uint8_t* right, uint32_t first, uint32_t avail,
                          song_pages_arp* a, uint32_t* rows);
void song_pages_arp_default(song_pages_arp* a);
int song_pages_arp_open(song_pages* pages, screen* s);
void song_pages_arp_save(song_pages* pages, screen* s);
int song_pages_arp_close(song_pages* pages);

int song_pages_batch_begin(song_pages* pages);
const char* song_pages_batch_rows(song_pages* pages, uint32_t table, uint32_t first, uint32_t n,
                                  uint32_t had, const uint8_t* left, const uint8_t* right);
const char* song_pages_batch_blank(song_pages* pages, uint32_t table, uint32_t first, uint32_t n);
const char* song_pages_batch_instr(song_pages* pages, uint32_t instr, const uint8_t bytes[GT2_INSTR_BYTES]);
int song_pages_batch_go(song_pages* pages);
int song_pages_batch_busy(const song_pages* pages);
const char* song_pages_lab_send(song_pages* pages, const uint8_t sound[LAB_SOUND_BYTES],
                                const char* name, uint32_t model, uint32_t* instr, uint32_t* notes);

void song_pages_table_meaning(uint32_t table, uint8_t left, uint8_t right, char out[31]);

extern const view song_pages_order_view;
extern const view song_pages_instr_view;
extern const view song_pages_table_view;

#ifdef __cplusplus
}
#endif

#endif
