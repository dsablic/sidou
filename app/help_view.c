// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "help_view.h"

#include <stddef.h>

#include "chip_view.h"
#include "grid.h"
#include "live.h"
#include "panel.h"
#include "song_view.h"
#include "theme.h"

/* What the controls do in each view and state, a line the control and what it does (a NUL
 * between), 14 characters with a space between the two, eight lines a page and two pages at
 * most (a host test checks). Each says what its own view does in that state
 * (docs/reference/controls.md), and every control the view's input takes is on it: a host test
 * (help_view.rs) feeds each view, in each state, every button and encoder and finds the line of
 * each one it takes. The control's words: SELECT, ALG (ALGORITHM), K1..K4 (K1-K4, K1 K2, K3/K4;
 * K alone is the four), PRESETS (PRE), KEYS, a button's name; E+ (or EDIT+) with EDIT held, STEP+
 * with a step held, GLO+, SAVE+, ARP+ (a digit after the NUL would be an octal escape). */
/* ARP switches the chord arpeggio at its release; held, K1..K3 its rate, order and span. The song
 * views' own cards say SEQ and GLO's mutes. */
static const char* const LIVE_LINES[] = {
    "SELECT\0PAGE",   "ALG\0PROFILE",  "K1\0CUTOFF",      "K2\0RESO",
    "K3\0SOUND'S OWN", "K4\0RELEASE",  "PRESETS\0SOUND",  "E+K\0SLOT",
    "SCL\0NEXT SLOT", "ENV LFO FX\0LAB", "ARP\0CHORD ARP", "ARP+K\0ARP SET",
    "OCT-/+\0OCTAVE", "SAVE+K\0LEARN",  "HOME\0MENU"};
/* LIVE on a slot's page: K1..K4 the slot's controls, named for its algorithm. */
static const char* const LIVE_SLOT_LINES[] = {
    "SELECT\0PAGE",   "ALG\0ALGORITHM", "K1\0\1",         "K2\0\2",
    "K3\0\3",         "K4\0\4",         "SCL\0NEXT SLOT", "ENV LFO FX\0LAB",
    "PRESETS\0SOUND", "OCT-/+\0OCTAVE", "GLO+KEY\0MUTE",
    "ARP\0CHORD ARP",  "SAVE+K\0LEARN",  "SEQ\0SONG VIEWS", "HOME\0MENU"};
/* SAVE is the menu's OK: it opens the view under the selection (ALG clockwise does too); the
 * settings save themselves. */
static const char* const MENU_LINES[] = {"SELECT\0MOVE", "SAVE\0OPEN", "ALG\0CHANGE",
                                         "ALG\0OPEN",    "HOME\0BACK", "HOLD HOME\0HELP"};
/* SETTINGS: values only; they save themselves a few seconds after a change. On STORE, ALG arms
 * the format of a foreign store and SAVE confirms it. SPREAD puts chip A left, chip B right. */
static const char* const SETTINGS_LINES[] = {"SELECT\0MOVE",  "ALG\0CHANGE", "SETTINGS\0KEPT",
                                             "SPREAD\0A/B L/R", "HOME\0BACK", "HOLD HOME\0HELP"};
static const char* const STORE_LINES[] = {"SELECT\0MOVE", "ALG\0ARM FORMAT", "SAVE\0FORMAT",
                                          "WIPES\0ALL DATA", "HOME\0BACK"};
/* RESET asks first (ALG), then SAVE does it: every preset's edits and setting back to factory. */
static const char* const RESET_LINES[] = {"SELECT\0MOVE", "ALG\0ARM RESET", "SAVE\0RESET",
                                          "KEEPS\0SONGS", "KEEPS\0PROFILES", "KEEPS\0MIDI MAP",
                                          "HOME\0BACK"};
/* The MIDI page has no actions, and its setup is RAM-only (not part of the saved settings). */
static const char* const MIDI_LINES[] = {"SELECT\0MOVE",   "ALG\0CHANGE", "SETUP\0RAM ONLY",
                                         "HOME\0BACK",     "HOLD HOME\0HELP"};
/* CHIP by its focus: the A/B tabs, the chip (K3, the thump, changes only where a note starts),
 * a slot (its algorithm's names) and EXPERT. */
static const char* const CHIP_TABS_LINES[] = {"SELECT\0FOCUS", "ALG\0CHIP A/B", "K1\0AGE",
                                              "K2\0DAC",       "K3\0NOTE THUMP", "K4\0BLEED",
                                              "E+K\0SLOT",     "HOME\0BACK"};
static const char* const CHIP_LINES[] = {"SELECT\0FOCUS",  "ALG\0PROFILE", "E+ALG\0SAVE TO",
                                         "K1\0AGE",        "K2\0DAC",      "K3\0NOTE THUMP",
                                         "K4\0BLEED",      "E+K\0SLOT",    "SAVE\0STORE",
                                         "HOME\0BACK"};
static const char* const CHIP_SLOT_LINES[] = {"SELECT\0FOCUS", "ALG\0ALGORITHM", "K1\0\1",
                                              "K2\0\2",        "K3\0\3",         "K4\0\4",
                                              "SCL\0NEXT SLOT", "HOME\0BACK"};
static const char* const EXPERT_LINES[] = {"SELECT\0FOCUS", "ALG\0PAGE",     "K1-K4\0AS SHOWN",
                                           "MACROS\0RESET", "E+ALG\0SAVE TO", "SAVE\0STORE",
                                           "E+K\0SLOT",     "HOME\0BACK"};
/* The song views: SEQ goes round them, SAVE saves the song, not the settings. STEP and TRACKER
 * undo one step (EDIT + LFO, E+LFO: the line's 14 characters; the ergonomics review's E32). STEP's
 * card is full: DICE's settings (K1-K4 held with EDIT + SCL), its root (a key) and the sweep
 * (E+ENV) took the places of PLAY, KEYS 17+ and GLO's mutes (every song view's, on TRACKER's). */
static const char* const STEP_LINES[] = {
    "KEYS 1-16\0STEP", "STEP+K\0LOCK",   "SELECT\0LANE",    "ALG\0PAGE",
    "PRESETS\0INSTR",  "ARP\0GATE",      "E+LFO\0ONE UNDO", "E+ARP\0MUTATE",
    "E+SCL\0DICE",     "SCL+K\0DICE SET", "SCL+KEY\0ROOT",   "E+ENV\0SWEEP",
    "REC\0RECORD",    "SEQ\0NEXT VIEW", "SAVE\0SONG",      "HOME\0BACK"};
/* A step held: its locks, its note; let go with no knob turned, it toggles. */
static const char* const STEP_HELD_LINES[] = {
    "K1\0CUTOFF LOCK", "K2\0RESO LOCK",  "K3\0PULSE LOCK", "K4\0WAVE SET",
    "E+K\0CHARACTER",  "PRESETS\0NOTE",  "LFO\0CLEAR LOCK", "KEY UP\0TOGGLE",
    "SELECT\0LANE",    "ALG\0CHANCE",    "ARP\0GATE",      "E+LFO\0ONE UNDO",
    "SEQ\0NEXT VIEW",  "SAVE\0SONG",     "HOME\0BACK"};
/* REC on while the song plays: the pads record, K1 and K2 lock on the row they turn in. */
static const char* const STEP_REC_LINES[] = {
    "KEYS\0RECORD",   "K1\0CUTOFF LOCK", "K2\0RESO LOCK",   "QUANT\0\5",
    "SELECT\0LANE",   "ALG\0PAGE",       "PRESETS\0INSTR",  "ARP\0GATE",
    "E+LFO\0ONE UNDO", "REC\0RECORD OFF", "PLAY\0STOP",     "SEQ\0NEXT VIEW",
    "SAVE\0SONG",     "HOME\0BACK"};
/* TRACKER's generative play is STEP's on the shown voice's pattern (app/song_gen.h): MUTATE,
 * DICE with its settings and root, the sweep; PLAY (every view's) gave its place. */
static const char* const TRACKER_LINES[] = {
    "SELECT\0ROW",     "E+SELECT\0PAGE", "ALG\0VOICE",     "E+ALG\0ROW STEP",
    "KEYS\0AUDITION",  "REC\0EDIT",      "E+LFO\0ONE UNDO", "E+ARP\0MUTATE",
    "E+SCL\0DICE",     "SCL+K\0DICE SET", "SCL+KEY\0ROOT",   "E+ENV\0SWEEP",
    "GLO+KEY\0MUTE",   "SEQ\0NEXT VIEW", "SAVE\0SONG",     "HOME\0BACK"};
/* REC on, the song stopped: the cursor's row is edited. DICE and the sweep (and MUTATE, on
 * TRACKER's card) work here too; EDIT's page and row step are on TRACKER's card. */
static const char* const TRACKER_EDIT_LINES[] = {
    "KEYS\0WRITE",     "ARP\0KEY OFF",   "LFO\0CLEAR NOTE", "K1\0INSTRUMENT",
    "K2\0COMMAND",     "K3 K4\0DATA",    "PRESETS\0INSTR",  "E+PRE\0INS/DEL",
    "SELECT\0ROW",     "ALG\0VOICE",     "E+SCL\0DICE",     "E+ENV\0SWEEP",
    "E+LFO\0ONE UNDO", "SEQ\0NEXT VIEW", "SAVE\0SONG",      "HOME\0BACK"};
/* REC on, the song playing: the keys and K1 and K2 record; EDIT + K1..K4 edit the cursor's row. */
static const char* const TRACKER_REC_LINES[] = {
    "KEYS\0RECORD",    "K1\0CUTOFF LOCK", "K2\0RESO LOCK",   "QUANT\0\5",
    "K3 K4\0DATA",     "E+K\0EDIT CELL",  "ARP\0KEY OFF",    "LFO\0CLEAR NOTE",
    "PRESETS\0INSTR",  "E+PRE\0INS/DEL",  "SELECT\0ROW",     "ALG\0VOICE",
    "E+LFO\0ONE UNDO", "SEQ\0NEXT VIEW",  "SAVE\0SONG",      "HOME\0BACK"};
static const char* const ORDER_LINES[] = {
    "SELECT\0ENTRY",  "ALG\0TUNE/VOICE", "K1\0VALUE",     "K2\0KIND",
    "E+PRE\0INS/DEL", "ARP\0RESTART",    "KEYS\0AUDITION", "PLAY\0FROM HERE",
    "SEQ\0NEXT VIEW", "SAVE\0SONG",      "HOME\0BACK"};
static const char* const INSTR_LINES[] = {
    "SELECT\0PARAM",  "PRESETS\0INSTR", "K1\0COARSE",    "K2\0FINE",
    "E+SEQ\0TABLE",   "KEYS\0AUDITION", "SEQ\0NEXT VIEW", "SAVE\0SONG",
    "HOME\0BACK"};
static const char* const TABLE_LINES[] = {"SELECT\0ROW",      "ALG\0TABLE",      "K1\0LEFT BYTE",
                                          "K2\0RIGHT BYTE",   "ARP\0ARPEGGIO",   "KEYS\0AUDITION",
                                          "SEQ\0NEXT VIEW",   "SAVE\0SONG",      "HOME\0BACK"};
/* TABLE's ARP helper: the arpeggio's steps, each a semitone offset, written as wavetable rows; the
 * keys play the draft. */
static const char* const ARP_LINES[] = {"SELECT\0STEP",   "ALG\0STEPS",    "K1 K2\0OFFSET",
                                        "K3\0SPEED",      "K4\0LOOP",      "PRESETS\0WAVE",
                                        "KEYS\0DRAFT", "SEQ\0NEXT VIEW", "SAVE\0WRITE",
                                        "HOME\0CANCEL"};
static const char* const SCENE_LINES[] = {"SELECT\0PIECE", "ALG\0PIECE", "K1 K2\0MOOD",
                                          "KEYS\0PLAY",    "HOME\0BACK"};
/* The SONG menu's actions; its pages have their own cards (below). SAVE is each page's OK, as
 * the menu's: the highlighted action, file or answer (NO goes back a page). */
static const char* const SONG_LINES[] = {"SELECT\0ACTION", "SAVE\0OPEN", "ALG\0OPEN",
                                         "SEQ\0SONG VIEWS", "HOME\0BACK"};
/* STATUS's SELECT turns its three pages; ABOUT and CHARSET take no encoder, and on all three the keys and
 * the knobs play as on LIVE. */
static const char* const STATUS_LINES[] = {"SELECT\0PAGE",  "KEYS\0PLAY",    "K1-K4\0SOUND",
                                           "SAVE\0KEEP",    "SEQ\0SONG",     "GLO+KEY\0MUTE",
                                           "HOME\0BACK",    "HOLD HOME\0HELP"};
/* (SEQ is refused over ABOUT's tune: no SEQ line) */
static const char* const ABOUT_LINES[] = {"KEYS\0PLAY", "SAVE\0KEEP", "HOME\0BACK",
                                          "HOLD HOME\0HELP"};
static const char* const CHARSET_LINES[] = {"KEYS\0PLAY", "SAVE\0KEEP", "SEQ\0SONG",
                                            "HOME\0BACK", "HOLD HOME\0HELP"};
/* The SONG menu's pages: LOAD's and DELETE's files, SAVE AS's name and a confirmation. HOME goes
 * back a page (to SONG's actions). */
static const char* const FILES_LINES[] = {"SELECT\0FILE", "SAVE\0CHOOSE", "ALG\0CHOOSE",
                                          "HOME\0BACK",   "HOLD HOME\0HELP"};
static const char* const NAME_LINES[] = {"SELECT\0CURSOR", "ALG\0CHARACTER", "SAVE\0WRITE",
                                         "HOME\0BACK",    "HOLD HOME\0HELP"};
static const char* const CONFIRM_LINES[] = {"SELECT\0NO/YES", "SAVE\0CHOOSE", "ALG\0CONFIRM",
                                            "HOME\0BACK",    "HOLD HOME\0HELP"};

/* The SID views (app/sid_view.h): SAVE is the browser's OK, as SONG's LOAD's; PLAY and EDIT +
 * PLAY are the tune's while one is loaded. NOW PLAYING's ALG steps the song; the keys play chip
 * B (the keys' chip under a tune). */
static const char* const SIDS_LINES[] = {"SELECT\0TUNE",    "SAVE\0CHOOSE",  "ALG\0CHOOSE",
                                         "PLAY\0PLAY/STOP", "E+PLAY\0PAUSE", "HOME\0BACK",
                                         "HOLD HOME\0HELP"};
static const char* const PLAYING_LINES[] = {"ALG\0SONG",      "PLAY\0PLAY/STOP", "E+PLAY\0PAUSE",
                                            "GLO+KEY\0MUTE",  "KEYS\0PLAY",      "HOME\0BACK",
                                            "HOLD HOME\0HELP"};
/* The question before a tune takes the song's memory: SAVE saves the song, ALG again discards
 * its changes, HOME keeps both. */
static const char* const UNSAVED_LINES[] = {"SAVE\0SAVE SONG", "ALG\0DISCARD", "HOME\0CANCEL",
                                            "HOLD HOME\0HELP"};

/* LAB's pages, a card each: the page's four cards and its choice first, then what every page
 * takes (two pages at most, so no line is spared: SAVE+K is MIDI learn, EDIT's tap is the hint
 * line's A/B). ENV, LFO and FX open their pages, EDIT + PRESETS reverts, HOME goes to LIVE. */
#define LAB_TAIL_A "SELECT\0PAGE", "ENV LFO\0PAGES", "FX\0PAGE"
#define LAB_TAIL_B "SAVE\0KEEP", "E+SAVE\0NAME", "E+LFO\0UNDO", "E+ARP\0VARY", "E+SCL\0ROLL", \
                   "E+PRE\0REVERT"
static const char* const LAB_OSC_LINES[] = {
    "K1\0PULSE/RATIO", "K2\0PWM DEPTH", "K3\0PWM RATE", "K4\0TUNE",
    "ALG\0WAVEFORM",   LAB_TAIL_A,      LAB_TAIL_B,     "SAVE+K\0LEARN", "HOME\0LIVE"};
static const char* const LAB_ENVELOPE_LINES[] = {
    "K1\0ATTACK", "K2\0DECAY", "K3\0SUSTAIN", "K4\0RELEASE",
    "ALG\0ENV MODE", LAB_TAIL_A, LAB_TAIL_B,   "SAVE+K\0LEARN", "HOME\0LIVE"};
static const char* const LAB_FILTER_LINES[] = {
    "K1\0CUTOFF", "K2\0RESONANCE", "K3\0VOICES", "K4\0UNUSED",
    "ALG\0FILTER", LAB_TAIL_A,      LAB_TAIL_B,   "SAVE+K\0LEARN", "HOME\0LIVE"};
static const char* const LAB_MOD_LINES[] = {
    "K1\0VIBRATO HZ", "K2\0VIB DEPTH", "K3\0VIB DELAY", "K4\0VELOCITY",
    "ALG\0ARP",       LAB_TAIL_A,      LAB_TAIL_B,      "SAVE+K\0LEARN", "HOME\0LIVE"};
/* FX: the selected slot's controls in its algorithm's words; SCL moves the slot (no room left
 * for the learn line). */
static const char* const LAB_FX_LINES[] = {
    "K1\0\1",       "K2\0\2",  "K3\0\3",    "K4\0\4",
    "ALG\0ALGORITHM", LAB_TAIL_A, LAB_TAIL_B, "SCL\0NEXT SLOT", "HOME\0LIVE"};
/* The name page (EDIT + SAVE): the keys type, the three page buttons and K2-K4 do nothing. */
static const char* const LAB_NAME_LINES[] = {
    "KEYS\0LETTERS", "SELECT\0CURSOR", "ALG\0CHARACTER", "K1\0SLOT",
    "SAVE\0WRITE",   "HOME\0BACK",     "K2-K4\0NOTHING", "ENV LFO FX\0OFF"};

#define CARD(name, lines) {name, lines, (uint8_t)(sizeof lines / sizeof lines[0])}
const help_card help_cards[HELP_CARDS] = {
    [HELP_CARD_LIVE] = CARD("LIVE", LIVE_LINES),       [HELP_CARD_MENU] = CARD("MENU", MENU_LINES),
    [HELP_CARD_CHIP] = CARD("CHIP", CHIP_LINES),       [HELP_CARD_TRACKER] = CARD("TRACKER", TRACKER_LINES),
    [HELP_CARD_SCENE] = CARD("SCENE", SCENE_LINES),    [HELP_CARD_STATUS] = CARD("STATUS", STATUS_LINES),
    [HELP_CARD_STEP] = CARD("STEP", STEP_LINES),       [HELP_CARD_ORDER] = CARD("ORDER", ORDER_LINES),
    [HELP_CARD_INSTR] = CARD("INSTR", INSTR_LINES),    [HELP_CARD_TABLE] = CARD("TABLE", TABLE_LINES),
    [HELP_CARD_MIDI] = CARD("MIDI", MIDI_LINES),       [HELP_CARD_SONG] = CARD("SONG", SONG_LINES),
    [HELP_CARD_SETTINGS] = CARD("SETTINGS", SETTINGS_LINES),
    [HELP_CARD_ABOUT] = CARD("ABOUT", ABOUT_LINES),    [HELP_CARD_CHARSET] = CARD("CHARSET", CHARSET_LINES),
    [HELP_CARD_FILES] = CARD("FILES", FILES_LINES),    [HELP_CARD_NAME] = CARD("SAVE AS", NAME_LINES),
    [HELP_CARD_CONFIRM] = CARD("CONFIRM", CONFIRM_LINES),
    [HELP_CARD_LIVE_SLOT] = CARD("LIVE SLOT", LIVE_SLOT_LINES),
    [HELP_CARD_CHIP_TABS] = CARD("CHIP TABS", CHIP_TABS_LINES),
    [HELP_CARD_CHIP_SLOT] = CARD("CHIP SLOT", CHIP_SLOT_LINES),
    [HELP_CARD_EXPERT] = CARD("EXPERT", EXPERT_LINES),
    [HELP_CARD_STEP_HELD] = CARD("HELD STEP", STEP_HELD_LINES),
    [HELP_CARD_STEP_REC] = CARD("STEP REC", STEP_REC_LINES),
    [HELP_CARD_TRACKER_EDIT] = CARD("TRACK EDIT", TRACKER_EDIT_LINES),
    [HELP_CARD_TRACKER_REC] = CARD("TRACK REC", TRACKER_REC_LINES),
    [HELP_CARD_STORE] = CARD("STORE", STORE_LINES),
    [HELP_CARD_ARP] = CARD("ARP", ARP_LINES),
    [HELP_CARD_RESET] = CARD("RESET", RESET_LINES),
    [HELP_CARD_SIDS] = CARD("SIDS", SIDS_LINES),
    [HELP_CARD_PLAYING] = CARD("PLAYING", PLAYING_LINES),
    [HELP_CARD_UNSAVED] = CARD("UNSAVED", UNSAVED_LINES),
    [HELP_CARD_LAB_OSC] = CARD("LAB OSC", LAB_OSC_LINES),
    [HELP_CARD_LAB_ENVELOPE] = CARD("LAB ENV", LAB_ENVELOPE_LINES),
    [HELP_CARD_LAB_FILTER] = CARD("LAB FILTER", LAB_FILTER_LINES),
    [HELP_CARD_LAB_MOD] = CARD("LAB MOD", LAB_MOD_LINES),
    [HELP_CARD_LAB_FX] = CARD("LAB FX", LAB_FX_LINES),
    [HELP_CARD_LAB_NAME] = CARD("LAB NAME", LAB_NAME_LINES),
};
_Static_assert(sizeof LIVE_LINES / sizeof LIVE_LINES[0] <= HELP_LINES * HELP_PAGES &&
                   sizeof TRACKER_EDIT_LINES / sizeof TRACKER_EDIT_LINES[0] <= HELP_LINES * HELP_PAGES &&
                   sizeof TRACKER_REC_LINES / sizeof TRACKER_REC_LINES[0] <= HELP_LINES * HELP_PAGES &&
                   sizeof TABLE_LINES / sizeof TABLE_LINES[0] <= HELP_LINES * HELP_PAGES &&
                   sizeof ARP_LINES / sizeof ARP_LINES[0] <= HELP_LINES * HELP_PAGES &&
                   sizeof LAB_OSC_LINES / sizeof LAB_OSC_LINES[0] <= HELP_LINES * HELP_PAGES &&
                   sizeof LAB_FX_LINES / sizeof LAB_FX_LINES[0] <= HELP_LINES * HELP_PAGES,
               "two pages at most");

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

/* A card's pages: two when its lines are more than a page's. */
static uint32_t pages(const help_card* c) {
    return c->count > HELP_LINES ? HELP_PAGES : 1u;
}

int help_view_capture(void* ctx, const screen_event* e) {
    help_view* h = ctx;
    if (e->kind == SCREEN_TURN && e->id == PANEL_ENC_SELECT && pages(&help_cards[h->card]) > 1u) {
        h->page ^= 1u; /* either way: two pages */
        h->drawn = 0;
        return 1;
    }
    h->done = 1;
    return 1;
}

int help_view_open(help_view* h, screen* s, uint32_t card, uint32_t arg) {
    h->drawn = 0;
    h->done = 0;
    h->page = 0;
    h->arg = (uint16_t)arg;
    h->card = (uint8_t)(card < HELP_CARDS ? card : HELP_CARD_STATUS);
    return screen_push_over(s, &help_view_view, h, SCREEN_CUT);
}

static void enter(screen* s, void* state) {
    screen_capture(s, help_view_capture, state);
}

const char* help_view_what(const help_view* h, const char* what, char* buf) {
    static const char* const QUANT[SONG_VIEW_QUANTS] = {"NEAREST", "OFF", "COUNT-IN"};
    _Static_assert(SONG_VIEW_QUANT_NEAREST == 0u && SONG_VIEW_QUANT_OFF == 1u &&
                       SONG_VIEW_QUANT_COUNT == 2u && CHIP_VIEW_LABEL <= HELP_TEXT_WORD,
                   "the quantise's names, a slot's word fits");
    int code = what[0];
    if (code >= HELP_TEXT_K1 && code <= HELP_TEXT_K4) {
        int used = chip_view_label(h->arg & 0xffu, h->arg >> 8,
                                   LIVE_SLOT_AMOUNT + (uint32_t)(code - HELP_TEXT_K1), buf);
        return used ? buf : "UNUSED";
    }
    if (code == HELP_TEXT_QUANT) {
        return QUANT[h->arg < SONG_VIEW_QUANTS ? h->arg : 0u];
    }
    return what;
}

/* Unit u of the page shown: 0 the title bar (HELP, or the page on a card of two, and the name),
 * then the page's lines. */
static void draw_unit(grid* g, const theme* th, const help_view* h, uint32_t u) {
    const help_card* c = &help_cards[h->card];
    const uint8_t blank = THEME_ATTR(th, value);
    if (u == 0) {
        const uint8_t head = GRID_ATTR(th->title_fg, th->title_bg);
        char page[4] = {(char)('1' + h->page), '/', (char)('0' + HELP_PAGES), 0};
        grid_fill(g, 0, 0, GRID_COLS, 3, ' ', head);
        grid_fill(g, 0, 3, GRID_COLS, 1, ' ', blank);
        grid_text_big(g, 1, 1, 2, pages(c) > 1u ? page : "HELP", head);
        grid_text_big(g, right(c->name), 1, 2, c->name, head);
        return;
    }
    uint32_t i = u - 1u;
    uint32_t row = HELP_ROW + HELP_LINE_ROWS * i;
    uint32_t line = h->page * HELP_LINES + i;
    grid_fill(g, 0, row, GRID_COLS, i == HELP_LINES - 1u ? HELP_LAST_ROWS : HELP_LINE_ROWS, ' ',
              blank);
    if (line < c->count) {
        char buf[HELP_TEXT_WORD];
        const char* key = c->lines[line];
        const char* what = help_view_what(h, key + length(key) + 1u, buf); /* after the NUL */
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
    uint32_t used = screen_pending(s) + screen_layer_cells(s); /* a layer's cells count too */
    for (uint32_t u = 0; u < HELP_UNITS; u++) {
        if (h->drawn >> u & 1u) {
            continue;
        }
        uint32_t cost = u == 0 ? HELP_TITLE_CELLS
                               : (u == HELP_UNITS - 1u ? HELP_LAST_ROWS * GRID_COLS : HELP_LINE_CELLS);
        if (used && used + cost > HELP_FRAME_CELLS) {
            break;
        }
        draw_unit(g, th, h, u);
        h->drawn |= (uint16_t)(1u << u);
        used += cost;
    }
}

const view help_view_view = {"HELP", enter, NULL, NULL, compose, changed};
