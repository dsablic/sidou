// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_SCREEN_H
#define SIDOU_SCREEN_H

#include <stdint.h>

#include "grid.h"
#include "theme.h"

#ifdef __cplusplus
extern "C" {
#endif

#define SCREEN_DEPTH 4u
#define SCREEN_SLOTS (SCREEN_DEPTH + 1u)
#define SCREEN_TOAST_ROW 27u
#define SCREEN_TOAST_COL 2u
#define SCREEN_TOAST_LINE 13u
#define SCREEN_TOAST_CHARS (2u * SCREEN_TOAST_LINE)
#define SCREEN_TOAST_MS 1500u
#define SCREEN_TOAST_FIRST_ROW(s) ((s)->toast_h == 4u ? SCREEN_TOAST_ROW - 2u : SCREEN_TOAST_ROW)
#define SCREEN_POPUP_COL 1u
#define SCREEN_POPUP_ROW 10u
#define SCREEN_POPUP_W 28u
#define SCREEN_POPUP_ROWS 9u
#define SCREEN_POPUP_STEP 3u
#define SCREEN_POPUP_MS 1000u
#define SCREEN_POPUP_BIG 8u
#define SCREEN_POPUP_CHARS 12u
#define SCREEN_POPUP_TEXT (SCREEN_POPUP_CHARS + 1u)
#define SCREEN_POPUP_BAR_CELLS 24u
#define SCREEN_BLINK_FRAMES 8u
#define SCREEN_WIPE_ROWS 4u
#define SCREEN_BAND_ROWS 6u
#define SCREEN_RASTER_FRAMES 2u
#define SCREEN_RAMP 7u

typedef enum {
    SCREEN_PRESS = 0,
    SCREEN_TURN = 1
} screen_event_kind;

typedef struct {
    uint8_t kind;
    uint8_t id;
    int16_t steps;
    uint32_t held;
} screen_event;

enum { SCREEN_LONE_NONE = 0, SCREEN_LONE_DONE = 1, SCREEN_LONE_HELD_BACK = 2 };
#define SCREEN_LONE_NO_BUTTON 0xFFu

typedef struct {
    uint8_t button;
    uint8_t waiting;
} screen_lone;

typedef struct screen screen;

typedef void (*screen_watch_fn)(void* ctx);

typedef int (*screen_popup_fn)(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill,
                               uint32_t* max, uint32_t* accent);

typedef struct {
    screen_popup_fn source;
    void* source_ctx;
    screen_watch_fn watch;
    void* watch_ctx;
    uint32_t until;
    uint8_t rows;
    uint8_t next;
    uint8_t alive;
    uint8_t fresh;
    uint8_t bar;
    uint8_t has_bar;
    uint8_t accent;
    const uint8_t* theme_id;
    char name[SCREEN_POPUP_TEXT];
    char value[SCREEN_POPUP_TEXT];
} screen_popup_state;

typedef struct {
    const char* name;
    void (*enter)(screen* s, void* state);
    void (*exit)(screen* s, void* state);
    int (*input)(screen* s, void* state, const screen_event* e);
    void (*compose)(screen* s, void* state, grid* g);
    int (*changed)(screen* s, void* state);
} view;

typedef enum {
    SCREEN_CUT = 0,
    SCREEN_WIPE_DOWN = 1,
    SCREEN_WIPE_UP = 2,
    SCREEN_OVER = 3,
    SCREEN_BANDS = 4
} screen_wipe;

typedef int (*screen_capture_fn)(void* ctx, const screen_event* e);

typedef struct {
    const view* v;
    void* state;
} screen_entry;

struct screen {
    grid* g;
    screen_entry stack[SCREEN_SLOTS];
    uint8_t depth;
    uint8_t wipe;
    uint8_t wipe_rows;
    uint8_t raster_frames;
    uint8_t toast_on;
    uint8_t redraw;
    uint8_t busy;
    uint8_t capture_depth;
    uint8_t layers;
    uint8_t toast_h;
    uint8_t toast_w;
    uint8_t toast_brk;
    uint32_t frame;
    uint32_t now;
    uint32_t redraw_rows;
    uint32_t toast_until;
    uint8_t cover_col, cover_row, cover_w, cover_h;
    uint16_t cover_cells;
    char toast[SCREEN_TOAST_CHARS + 1u];
    screen_capture_fn capture;
    void* capture_ctx;
    screen_capture_fn global;
    void* global_ctx;
    uint8_t popup_ask;
    uint8_t composed;
    uint8_t popup_defer;
    const view* popup_view;
    screen_popup_state* popup;
};

extern const uint8_t screen_ramp[SCREEN_RAMP];

void screen_init(screen* s, grid* g, const view* root, void* state);
int screen_push(screen* s, const view* v, void* state, screen_wipe w);
int screen_push_over(screen* s, const view* v, void* state, screen_wipe w);
int screen_pop(screen* s, screen_wipe w);
static inline const view* screen_top(const screen* s) {
    return s->stack[s->depth - 1u].v;
}
void screen_capture(screen* s, screen_capture_fn fn, void* ctx);
void screen_global(screen* s, screen_capture_fn fn, void* ctx);
int screen_input(screen* s, const screen_event* e);
void screen_reveal(screen* s, screen_wipe w);
void screen_redraw(screen* s);
void screen_toast(screen* s, const char* text, uint32_t ms);
uint32_t screen_toast_take(screen* s);
void screen_popup_source(screen* s, screen_popup_state* st, screen_popup_fn fn, void* ctx);
void screen_theme_source(screen* s, const uint8_t* id);
const theme* screen_theme(const screen* s);
void screen_popup_watch(screen* s, screen_watch_fn fn, void* ctx);
void screen_popup_ask(screen* s, uint32_t ask);
void screen_popup(screen* s, const char* name, const char* value, uint32_t fill, uint32_t max);
void screen_popup_accent(screen* s, uint32_t accent);
int screen_popup_shows(const screen* s);
void screen_lone_wait(screen_lone* l, uint32_t button, uint32_t waiting);
uint32_t screen_lone_chord(screen_lone* l, const screen_event* e);
uint32_t screen_lone_take(screen_lone* l);
uint32_t screen_lone_released(screen_lone* l, uint32_t held);
extern const char screen_note_names[12][2];
void screen_note_text(uint32_t note, char out[4]);
uint32_t screen_decimal(char* out, uint32_t n);
void screen_popup_copy(char out[SCREEN_POPUP_TEXT], const char* text);
void screen_raster(screen* s, uint32_t steps);
int screen_blink(const screen* s);
int screen_animating(const screen* s);
uint32_t screen_reach(const screen* s);
uint32_t screen_layer_cells(const screen* s);
void screen_cover(screen* s, uint32_t col, uint32_t row, uint32_t w, uint32_t h, uint32_t cells);
int screen_covered(const screen* s, uint32_t col, uint32_t row);
uint32_t screen_pending(const screen* s);
void screen_frame(screen* s, uint32_t now_ms);

#ifdef __cplusplus
}
#endif

#endif
