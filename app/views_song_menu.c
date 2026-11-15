// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

int song_refused(views* v) {
    uint32_t tune = sid_view_state(&v->sids);
    if (v->tune || tune != SID_T_NONE) {
        if (tune == SID_T_STOPPED || tune == SID_T_FAILED || tune == SID_T_ENDING) {
            (void)v->sids.t->end(v->sids.t->ctx);
            screen_toast(&v->s, "LOADING", SCREEN_TOAST_MS);
        } else {
            screen_toast(&v->s, "SID PLAYING: STOP TO EDIT", SCREEN_TOAST_MS);
        }
        return 1;
    }
    if (!v->song.io || !(v->song.io->about || song_view_loading(&v->song))) {
        return 0;
    }
    screen_toast(&v->s, screen_top(&v->s) == &about_view ? "LEAVE ABOUT FIRST" : "LOADING",
                 SCREEN_TOAST_MS);
    return 1;
}

enum { SONG_MENU_LOAD, SONG_MENU_NEW, SONG_MENU_SAVE, SONG_MENU_DELETE };
#define SONG_MENU_CANVAS SONG_VIEW_CANVAS_MENU

static const char* const SONG_ACTIONS[] = {"LOAD", "NEW", "SAVE AS", "DELETE"};
static int song_menu_input(void* ctx, const screen_event* e);
static int song_menu_global(void* ctx, const screen_event* e) {
    return song_menu_input(ctx, e) || global(ctx, e);
}

static void song_menu_changed(views* v) {
    v->songs.built = 0;
}

static const char SONG_BUILTIN[] = {'S', 'I', 'D', 'O', 'U', ' ', 'L', 'O', 'O', 'P', ' ',
                                    (char)FONT_MIDDOT, 0};

static int song_builtin_listed(const views* v) {
    return v->songs.action == SONG_MENU_LOAD && v->song.io && v->song.io->hooks.builtin;
}

static void song_directory(views* v, uint8_t page) {
    views_song_menu* m = &v->songs;
    uint32_t builtin = (uint32_t)song_builtin_listed(v);
    m->page = SONG_MENU_DIR;
    m->directory_page = page;
    m->cursor = 0;
    m->count = 0;
    m->after[0] = 0;
    m->skip = (uint8_t)(page * VIEWS_SONG_FILES - (page ? builtin : 0u));
    if (builtin && !page) {
        views_join(m->files[m->count++], sizeof m->files[0], SONG_BUILTIN, "");
    }
    m->scanning = 1;
    song_menu_changed(v);
}

void song_name(views* v, int direct) {
    views_song_menu* m = &v->songs;
    const song_io* io = v->song.io;
    const char* name = io && io->name[0] ? io->name
                       : io && io->hooks.builtin && !io->dirty ? SONG_IO_BUILTIN_NAME
                                                               : "NEW SONG";
    uint32_t n = 0;
    while (name[n] && n < STORE_NAME_MAX) {
        m->name[n] = name[n];
        n++;
    }
    while (n < STORE_NAME_MAX) {
        m->name[n++] = ' ';
    }
    m->name[STORE_NAME_MAX] = 0;
    m->page = SONG_MENU_NAME;
    m->action = SONG_MENU_SAVE;
    m->cursor = 0;
    m->direct = (uint8_t)direct;
    m->return_view = NULL;
    m->scanning = 0;
    song_menu_changed(v);
}

static void song_menu_done(views* v) {
    uint8_t direct = v->songs.direct;
    if (direct) {
        const view* back = v->songs.return_view;
        screen_pop(&v->s, SCREEN_WIPE_UP);
        v->songs.direct = 0;
        v->songs.page = SONG_MENU_ROOT;
        v->songs.cursor = SONG_MENU_SAVE;
        v->songs.scanning = 0;
        song_menu_changed(v);
        if (direct == 2u) {
            screen_push(&v->s, back, song_state(v, back), SCREEN_WIPE_UP);
        }
    } else {
        v->songs.page = SONG_MENU_ROOT;
        v->songs.cursor = v->songs.action;
        v->songs.scanning = 0;
        song_menu_changed(v);
    }
}

static void song_menu_execute(views* v, int confirmed) {
    views_song_menu* m = &v->songs;
    song_io* io = v->song.io;
    if (!io) {
        screen_toast(&v->s, "NO STORE", SCREEN_TOAST_MS);
        return;
    }
    if (song_io_busy(io)) {
        screen_toast(&v->s, song_io_saving(io) ? "SAVING" : "SONG BUSY", SCREEN_TOAST_MS);
        return;
    }
    if (!confirmed && ((song_io_changed(io) && (m->action == SONG_MENU_LOAD || m->action == SONG_MENU_NEW)) ||
                       m->action == SONG_MENU_DELETE)) {
        m->page = SONG_MENU_CONFIRM;
        song_menu_changed(v);
        return;
    }
    int accepted;
    switch (m->action) {
    case SONG_MENU_LOAD:
        accepted = strcmp(m->name, SONG_BUILTIN) == 0 ? song_io_load_builtin(io, confirmed)
                                                      : song_io_load(io, m->name, confirmed);
        break;
    case SONG_MENU_NEW: accepted = song_io_new(io, confirmed); break;
    case SONG_MENU_SAVE: accepted = song_io_save(io, m->name, v->s.now); break;
    case SONG_MENU_DELETE: accepted = song_io_delete(io, m->name, confirmed); break;
    default: return;
    }
    if (accepted) {
        song_menu_done(v);
    }
}

static void song_menu_save_name(views* v) {
    views_song_menu* m = &v->songs;
    uint32_t n = STORE_NAME_MAX;
    while (n && m->name[n - 1u] == ' ') {
        n--;
    }
    m->name[n] = 0;
    if (!n || m->name[0] == ' ') {
        screen_toast(&v->s, "NAME NEEDS FIRST CHARACTER", SCREEN_TOAST_MS);
        while (n < STORE_NAME_MAX) {
            m->name[n++] = ' ';
        }
        return;
    }
    song_menu_execute(v, 0);
    if (screen_top(&v->s) == &views_song_menu_view && m->page == SONG_MENU_NAME) {
        while (n < STORE_NAME_MAX) {
            m->name[n++] = ' ';
        }
    }
}

static int32_t song_clamp(int32_t n, int32_t max) {
    return n < 0 ? 0 : n > max ? max : n;
}

int song_menu_back(views* v) {
    views_song_menu* m = &v->songs;
    if (m->page == SONG_MENU_NAME && m->direct) {
        song_menu_done(v);
        return 1;
    }
    if (m->page == SONG_MENU_ROOT) {
        return 0;
    }
    if (m->page == SONG_MENU_CONFIRM && m->action != SONG_MENU_NEW) {
        m->page = m->action == SONG_MENU_SAVE ? SONG_MENU_NAME : SONG_MENU_DIR;
    } else {
        m->page = SONG_MENU_ROOT;
        m->cursor = m->action;
        m->scanning = 0;
    }
    song_menu_changed(v);
    return 1;
}

static void song_menu_open(views* v) {
    views_song_menu* m = &v->songs;
    if (m->page == SONG_MENU_ROOT) {
        m->action = m->cursor;
        if (m->action == SONG_MENU_LOAD || m->action == SONG_MENU_DELETE) {
            song_directory(v, 0);
        } else if (m->action == SONG_MENU_SAVE) {
            song_name(v, 0);
        } else {
            song_menu_execute(v, 0);
        }
    } else if (m->page == SONG_MENU_DIR && !m->scanning && m->count) {
        views_join(m->name, sizeof m->name, m->files[m->cursor], "");
        song_menu_execute(v, 0);
    } else if (m->page == SONG_MENU_NAME) {
        song_menu_save_name(v);
    } else if (m->page == SONG_MENU_CONFIRM) {
        song_menu_execute(v, 1);
    }
}

static int song_menu_input(void* ctx, const screen_event* e) {
    views* v = ctx;
    views_song_menu* m = &v->songs;
    if (screen_top(&v->s) != &views_song_menu_view) {
        return 0;
    }
    if (e->kind != SCREEN_TURN || (e->id != PANEL_ENC_SELECT && e->id != PANEL_ENC_ALGORITHM)) {
        return 0;
    }
    if (m->page == SONG_MENU_CONFIRM) {
        return 1;
    }
    int32_t steps = e->steps;
    if (e->id == PANEL_ENC_SELECT) {
        if (m->page == SONG_MENU_DIR) {
            int32_t next = (int32_t)m->cursor + steps;
            if (!m->scanning && next >= (int32_t)VIEWS_SONG_FILES && m->count == VIEWS_SONG_FILES &&
                (uint32_t)(m->directory_page + 1u) * VIEWS_SONG_FILES < STORE_KEYS_MAX) {
                song_directory(v, (uint8_t)(m->directory_page + 1u));
            } else if (next < 0 && m->directory_page) {
                song_directory(v, (uint8_t)(m->directory_page - 1u));
            } else {
                m->cursor = (uint8_t)song_clamp(next, m->count ? m->count - 1 : 0);
            }
        } else {
            m->cursor = (uint8_t)song_clamp((int32_t)m->cursor + steps,
                                           m->page == SONG_MENU_NAME ? STORE_NAME_MAX : 3u);
        }
    } else if (m->page == SONG_MENU_NAME && m->cursor < STORE_NAME_MAX) {
        uint32_t c = m->cursor;
        int32_t lo = c == 0 ? '!' : ' ';
        int32_t range = 0x60 - lo;
        int32_t ch = ((int32_t)m->name[c] - lo + steps) % range;
        m->name[c] = (char)(lo + (ch < 0 ? ch + range : ch));
    } else if (steps > 0) {
        song_menu_open(v);
    }
    song_menu_changed(v);
    return 1;
}

void song_menu_ok(views* v) {
    views_song_menu* m = &v->songs;
    if (m->page == SONG_MENU_NAME) {
        song_menu_save_name(v);
    } else {
        song_menu_open(v);
    }
    song_menu_changed(v);
}

static void song_menu_enter(screen* s, void* state) {
    views* v = state;
    song_menu_changed(v);
    screen_global(s, song_menu_global, v);
}

static void song_menu_exit(screen* s, void* state) {
    views* v = state;
    v->song.built = 0;
    screen_global(s, global, v);
    for (uint32_t i = 0; i + 1u < s->depth; i++) {
        if (s->stack[i].v == &views_song_menu_view) {
            screen_global(s, song_menu_global, v);
            break;
        }
    }
}

static void song_directory_step(views* v) {
    views_song_menu* m = &v->songs;
    store_file file;
    memset(&file, 0, sizeof file);
    views_join(file.name, sizeof file.name, m->after, "");
    if (!v->song.io || !v->song.io->st || !store_dir_next(v->song.io->st, &file)) {
        m->scanning = 0;
        return;
    }
    views_join(m->after, sizeof m->after, file.name, "");
    if (!song_io_listed(&file)) {
        return;
    }
    if (m->skip) {
        m->skip--;
    } else {
        views_join(m->files[m->count++], sizeof m->files[0], file.name, "");
        if (m->count == VIEWS_SONG_FILES) {
            m->scanning = 0;
        }
    }
}

static void song_canvas_text(views* v, uint32_t row, uint32_t col, const char* text, uint8_t a) {
    while (*text && col < GRID_COLS) {
        v->song.cv.ch[row][col] = (uint8_t)*text++;
        v->song.cv.attr[row][col++] = a;
    }
}

static void song_canvas_big(views* v, uint32_t row, uint32_t col, const char* text, uint8_t a) {
    while (*text && col + 1u < GRID_COLS) {
        uint8_t ch = (uint8_t)*text++;
        for (uint32_t y = 0; y < 2u; y++) {
            for (uint32_t x = 0; x < 2u; x++) {
                v->song.cv.ch[row + y][col + x] = ch;
                v->song.cv.attr[row + y][col + x] = a;
            }
        }
        col += 2u;
    }
}

static void song_canvas_name(views* v, uint32_t row, const char* name, uint8_t a) {
    char line[SONG_VIEW_NAME_CELLS + 1u];
    song_view_name_cells(name, line);
    song_canvas_big(v, row, 2, line, a);
}

static void song_canvas_ask_big(void* ctx, uint32_t row, const char* text, uint8_t a) {
    song_canvas_big(ctx, row, 2, text, a);
}

static uint32_t song_menu_big_rows(const views_song_menu* m) {
    uint32_t rows = 3u << 2;
    rows |= 3u << 4;
    if (m->page == SONG_MENU_ROOT) {
        for (uint32_t i = 0; i < 4u; i++) {
            rows |= 3u << (6u + i * 4u);
        }
    } else if (m->page == SONG_MENU_NAME) {
        rows |= (3u << 8) | (3u << 12) | (3u << 16) | (3u << 18) | (3u << 20) | (3u << 22);
    } else if (m->page == SONG_MENU_DIR) {
        rows |= ((1u << (2u * VIEWS_SONG_FILES + 2u)) - 1u) << 6;
    } else if (m->page == SONG_MENU_CONFIRM) {
        rows |= (3u << SONG_VIEW_ASK_ROW) | (3u << (SONG_VIEW_ASK_ROW + 2u)) |
                (3u << SONG_VIEW_ASK_NAME_ROW) | (3u << SONG_VIEW_ASK_AGAIN_ROW);
    }
    if (m->page != SONG_MENU_NAME) {
        rows |= 3u << 24;
    }
    rows |= 3u << 26;
    return rows;
}

static uint8_t song_menu_io_state(const views* v) {
    return v->song.io ? v->song.io->state : 0;
}

static int song_menu_dirty(screen* s, void* state) {
    (void)s;
    const views* v = state;
    return !v->songs.built || v->songs.scanning || v->song.dirty ||
           v->song.which != SONG_MENU_CANVAS || v->songs.io_state != song_menu_io_state(v) ||
           v->songs.drawn_theme != v->l->theme;
}

static void song_menu_compose(screen* s, void* state, grid* g) {
    views* v = state;
    views_song_menu* m = &v->songs;
    if (song_view_acquire_canvas(&v->song, SONG_MENU_CANVAS)) {
        m->built = 0;
    }
    const theme* th = theme_get(v->l->theme);
    const uint8_t text = THEME_ATTR(th, menu_item);
    const uint8_t label = THEME_ATTR(th, label);
    const uint8_t legend = THEME_ATTR(th, hint);
    const uint8_t value = THEME_ATTR(th, value);
    const uint8_t hot = THEME_ATTR(th, hot);
    const uint8_t alert = THEME_ATTR(th, alert);
    const uint8_t selected = GRID_ATTR(th->sel_fg, th->sel_bg);
    if (m->scanning && m->page == SONG_MENU_DIR) {
        song_directory_step(v);
        m->built = 0;
    }
    if (!m->built || v->song.which != SONG_MENU_CANVAS || m->io_state != song_menu_io_state(v) ||
        m->drawn_theme != v->l->theme) {
        for (uint32_t r = 0; r < GRID_ROWS; r++) {
            for (uint32_t c = 0; c < GRID_COLS; c++) {
                v->song.cv.ch[r][c] = ' ';
                v->song.cv.attr[r][c] = text;
            }
        }
        song_canvas_big(v, 2, 2, m->page == SONG_MENU_ROOT ? "SONG" : SONG_ACTIONS[m->action],
                        GRID_ATTR(th->title_fg, th->title_bg));
        if (v->song.io && song_io_busy(v->song.io)) {
            song_canvas_big(v, 4, 2, song_io_saving(v->song.io) ? "SAVING" : "BUSY", hot);
        }
        if (m->page == SONG_MENU_ROOT) {
            for (uint32_t i = 0; i < 4u; i++) {
                song_canvas_big(v, 6u + i * 4u, 2, SONG_ACTIONS[i], i == m->cursor ? selected : text);
            }
        } else if (m->page == SONG_MENU_DIR) {
            for (uint32_t i = 0; i < m->count; i++) {
                song_canvas_name(v, 6u + i * 2u, m->files[i], i == m->cursor ? selected : text);
            }
            char page[] = {'P', 'A', 'G', 'E', ' ', (char)('1' + m->directory_page), ' ',
                           m->directory_page ? (char)FONT_ARROW_UP : ' ',
                           m->count == VIEWS_SONG_FILES ? (char)FONT_ARROW_DOWN : ' ', 0};
            song_canvas_big(v, 22, 2,
                            m->scanning ? "READING" : !m->count ? "NO SONGS"
                            : m->directory_page || m->count == VIEWS_SONG_FILES ? page : "",
                            label);
        } else if (m->page == SONG_MENU_NAME) {
            char half[9];
            for (uint32_t i = 0; i < 2u; i++) {
                memcpy(half, &m->name[i * 8u], 8);
                half[8] = 0;
                song_canvas_big(v, 8u + i * 4u, 2, half, value);
            }
            if (m->cursor < STORE_NAME_MAX) {
                song_canvas_text(v, 10u + m->cursor / 8u * 4u, 2u + m->cursor % 8u * 2u, "^^", hot);
            }
            song_canvas_big(v, 16, 2, "SAVE", m->cursor == STORE_NAME_MAX ? selected : text);
            song_canvas_big(v, 18, 2, "SELECT CURSOR", legend);
            song_canvas_big(v, 20, 2, "ALG CHARACTER", legend);
            song_canvas_big(v, 22, 2, "SAVE WRITE", legend);
        } else {
            const char* name = m->action == SONG_MENU_NEW ? NULL : m->name;
            if (m->action == SONG_MENU_DELETE) {
                song_view_ask(song_canvas_ask_big, v, "DELETE SONG?", NULL, name, alert, value);
            } else {
                song_view_ask_discard(song_canvas_ask_big, v, name, alert, value);
            }
        }
        if (m->page == SONG_MENU_ROOT || m->page == SONG_MENU_DIR) {
            song_canvas_big(v, 24, 2, m->page == SONG_MENU_DIR ? "SAVE CHOOSE" : "SAVE OPEN", legend);
        }
        song_canvas_big(v, 26, 2, "HOME BACK", legend);
        v->song.dirty = (1u << GRID_ROWS) - 1u;
        m->built = 1;
        m->io_state = song_menu_io_state(v);
        m->drawn_theme = v->l->theme;
    }
    song_view_copy_large(&v->song, s, g, song_menu_big_rows(m));
}

const view views_song_menu_view = {"SONG", song_menu_enter, song_menu_exit, NULL,
                                   song_menu_compose, song_menu_dirty};

void song_open(void* ctx, int32_t steps) {
    (void)steps;
    views* v = ctx;
    if (v->tune) {
        (void)song_refused(v);
        return;
    }
    v->songs.page = SONG_MENU_ROOT;
    v->songs.cursor = 0;
    v->songs.direct = 0;
    v->songs.scanning = 0;
    screen_push(&v->s, &views_song_menu_view, v, SCREEN_WIPE_DOWN);
}

