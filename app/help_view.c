// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "help_view.h"

#include <stddef.h>

#include "grid.h"
#include "theme.h"

/* What the controls do in each view, a line the control and what it does (a NUL between), 14
 * characters with a space between the two (a host test checks). Each says what its own view does
 * (docs/reference/controls.md; a test opens every card over its view). */
static const char* const LIVE_LINES[] = {"SELECT\0PAGE",     "ALG\0PROF/SLOT", "PRESETS\0SOUND",
                                         "EDIT+K\0SLOT",     "SCL/ENV\0SLOT",  "GLO+KEY\0MUTE",
                                         "SAVE+K\0LEARN",    "HOME\0MENU"};
static const char* const MENU_LINES[] = {"SELECT\0MOVE", "ALG\0CHANGE", "ALG\0OPEN",
                                         "SAVE\0KEEP",   "HOME\0BACK",  "HOLD HOME\0HELP"};
/* The MIDI page has no actions, and its setup is RAM-only (not part of the saved settings). */
static const char* const MIDI_LINES[] = {"SELECT\0MOVE",   "ALG\0CHANGE", "SETUP\0RAM ONLY",
                                         "HOME\0BACK",     "HOLD HOME\0HELP"};
static const char* const CHIP_LINES[] = {"SELECT\0FOCUS", "ALG\0STEP",    "K1-K4\0VALUES",
                                         "EDIT+ALG\0SAVE", "SAVE\0STORE", "HOME\0BACK"};
/* The song views: SEQ goes round them, SAVE saves the song, not the settings. */
static const char* const STEP_LINES[] = {"SELECT\0LANE",   "ALG\0PAGE",      "KEYS 1-16\0STEP",
                                         "STEP+K\0LOCK",   "PRESETS\0INSTR", "ARP\0GATE",
                                         "SEQ\0NEXT VIEW", "SAVE\0SONG"};
static const char* const TRACKER_LINES[] = {"SELECT\0ROW",    "ALG\0VOICE",     "KEYS\0AUDITION",
                                            "REC\0RECORD",    "GLO+KEY\0MUTE",  "SEQ\0NEXT VIEW",
                                            "SAVE\0SONG",     "HOME\0BACK"};
static const char* const ORDER_LINES[] = {"SELECT\0ENTRY",    "ALG\0TUNE/VOICE", "K1\0VALUE",
                                          "K2\0KIND",         "ARP\0RESTART",    "PLAY\0FROM HERE",
                                          "SEQ\0NEXT VIEW",   "SAVE\0SONG"};
static const char* const INSTR_LINES[] = {"SELECT\0PARAM",    "PRESETS\0INSTR",  "K1\0COARSE",
                                          "K2\0FINE",         "EDIT+SEQ\0TABLE", "SEQ\0NEXT VIEW",
                                          "SAVE\0SONG",       "HOME\0BACK"};
static const char* const TABLE_LINES[] = {"SELECT\0ROW",      "ALG\0TABLE",      "K1\0LEFT BYTE",
                                          "K2\0RIGHT BYTE",   "SEQ\0NEXT VIEW",  "SAVE\0SONG",
                                          "HOME\0BACK"};
static const char* const SCENE_LINES[] = {"SELECT\0PIECE", "ALG\0PIECE", "K1 K2\0MOOD",
                                          "KEYS\0PLAY",    "HOME\0BACK"};
/* The SONG menu's root: the card opens there only (HOME acts on its press on every page). */
static const char* const SONG_LINES[] = {"SELECT\0ACTION", "ALG\0OPEN", "SEQ\0SONG VIEWS",
                                         "HOME\0BACK"};
static const char* const ALL_LINES[] = {"HOME\0BACK", "SAVE\0KEEP", "SEQ\0SONG",
                                        "GLO+KEY\0MUTE", "HOLD HOME\0HELP"};

#define CARD(name, lines) {name, lines, (uint8_t)(sizeof lines / sizeof lines[0])}
const help_card help_cards[HELP_CARDS] = {
    [HELP_CARD_LIVE] = CARD("LIVE", LIVE_LINES),       [HELP_CARD_MENU] = CARD("MENU", MENU_LINES),
    [HELP_CARD_CHIP] = CARD("CHIP", CHIP_LINES),       [HELP_CARD_TRACKER] = CARD("TRACKER", TRACKER_LINES),
    [HELP_CARD_SCENE] = CARD("SCENE", SCENE_LINES),    [HELP_CARD_ALL] = CARD("ALL", ALL_LINES),
    [HELP_CARD_STEP] = CARD("STEP", STEP_LINES),       [HELP_CARD_ORDER] = CARD("ORDER", ORDER_LINES),
    [HELP_CARD_INSTR] = CARD("INSTR", INSTR_LINES),    [HELP_CARD_TABLE] = CARD("TABLE", TABLE_LINES),
    [HELP_CARD_MIDI] = CARD("MIDI", MIDI_LINES),       [HELP_CARD_SONG] = CARD("SONG", SONG_LINES),
};

/* The text rows: the title bar's three and the row after it (unit 0), then a line every three
 * rows from row 4 (two rows of text and one blank); the last line owns the rows under it too. */
#define HELP_ROW 4u
#define HELP_LINE_ROWS 3u
#define HELP_TITLE_CELLS (4u * GRID_COLS)
#define HELP_LINE_CELLS (HELP_LINE_ROWS * GRID_COLS)
#define HELP_LAST_ROWS (GRID_ROWS - HELP_ROW - HELP_LINE_ROWS * (HELP_LINES - 1u))

static uint32_t length(const char* s) {
    uint32_t n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

/* Text of n characters at 2x ends at the margin: column 1 + 28 on the right. */
static uint32_t right(const char* s) {
    return GRID_COLS - 1u - 2u * length(s);
}

int help_view_capture(void* ctx, const screen_event* e) {
    (void)e;
    ((help_view*)ctx)->done = 1;
    return 1;
}

int help_view_open(help_view* h, screen* s, uint32_t card) {
    h->drawn = 0;
    h->done = 0;
    h->card = (uint8_t)(card < HELP_CARDS ? card : HELP_CARD_ALL);
    return screen_push(s, &help_view_view, h, SCREEN_CUT);
}

static void enter(screen* s, void* state) {
    screen_capture(s, help_view_capture, state);
}

static void draw_unit(grid* g, const theme* th, const help_card* c, uint32_t u) {
    const uint8_t blank = THEME_ATTR(th, value);
    if (u == 0) {
        const uint8_t head = GRID_ATTR(th->title_fg, th->title_bg);
        grid_fill(g, 0, 0, GRID_COLS, 3, ' ', head);
        grid_fill(g, 0, 3, GRID_COLS, 1, ' ', blank);
        grid_text_big(g, 1, 1, 2, "HELP", head);
        grid_text_big(g, right(c->name), 1, 2, c->name, head);
        return;
    }
    uint32_t i = u - 1u;
    uint32_t row = HELP_ROW + HELP_LINE_ROWS * i;
    grid_fill(g, 0, row, GRID_COLS, i == HELP_LINES - 1u ? HELP_LAST_ROWS : HELP_LINE_ROWS, ' ',
              blank);
    if (i < c->count) {
        const char* key = c->lines[i];
        const char* what = key + length(key) + 1u; /* after the NUL */
        grid_text_big(g, 1, row, 2, key, GRID_ATTR(th->hot, th->bg));
        grid_text_big(g, right(what), row, 2, what, blank);
    }
}

static int changed(screen* s, void* state) {
    (void)s;
    return ((const help_view*)state)->drawn != (1u << HELP_UNITS) - 1u;
}

/* The units not on the grid yet, in order, while the frame stays within HELP_FRAME_CELLS (a
 * unit is all or nothing; one always goes out). Whatever drew over it, or a view switch, has
 * them drawn again. */
static void compose(screen* s, void* state, grid* g) {
    help_view* h = state;
    if (s->redraw) {
        h->drawn = 0;
    }
    const theme* th = screen_theme(s);
    const help_card* c = &help_cards[h->card];
    uint32_t used = screen_pending(s);
    for (uint32_t u = 0; u < HELP_UNITS; u++) {
        if (h->drawn >> u & 1u) {
            continue;
        }
        uint32_t cost = u == 0 ? HELP_TITLE_CELLS
                               : (u == HELP_UNITS - 1u ? HELP_LAST_ROWS * GRID_COLS : HELP_LINE_CELLS);
        if (used && used + cost > HELP_FRAME_CELLS) {
            break;
        }
        draw_unit(g, th, c, u);
        h->drawn |= (uint16_t)(1u << u);
        used += cost;
    }
}

const view help_view_view = {"HELP", enter, NULL, NULL, compose, changed};
