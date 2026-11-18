// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

const char* const SLOT_NAMES[LIVE_SLOTS] = {"DIST", "MOD", "FX"};
static const char* const DIST_NAMES[] = {"OFF", "TAPE", "OVERDRIVE", "FUZZ", "CRUSH", "LO-FI"};
static const char* const MOD_NAMES[] = {"OFF",     "CHORUS",   "FLANGER", "PHASER",
                                        "TREMOLO", "AUTO-PAN", "WOW"};
static const char* const FX_NAMES[] = {"OFF", "DELAY", "PING-PONG", "REVERB", "LUSH"};

const char* views_algorithm_name(uint32_t slot, uint32_t alg) {
    static const char* const* const NAMES[LIVE_SLOTS] = {DIST_NAMES, MOD_NAMES, FX_NAMES};
    return slot < LIVE_SLOTS && alg < live_slot_algorithms(slot) ? NAMES[slot][alg] : "?";
}

const char* views_slot_name(uint32_t slot) {
    return slot < LIVE_SLOTS ? SLOT_NAMES[slot] : "?";
}

void views_put(grid* g, uint8_t attr, uint32_t row, uint32_t col, const char* s) {
    while (*s && col < GRID_COLS - 1u) {
        grid_put(g, col++, row, (uint8_t)*s++, attr);
    }
}

void views_append(char* buf, uint32_t cap, const char* s) {
    uint32_t n = grid_text_length(buf);
    while (*s && n + 1u < cap) {
        buf[n++] = *s++;
    }
    buf[n] = 0;
}

void views_join(char* buf, uint32_t cap, const char* a, const char* b) {
    buf[0] = 0;
    views_append(buf, cap, a);
    views_append(buf, cap, b);
}

__attribute__((noinline, unused)) void views_profile_label(char* buf, uint32_t id) {
    const sid_chip_profile* p = sid_chip_profile_get(id);
    views_join(buf, SCREEN_POPUP_TEXT, p && p->model == SID_MODEL_8580 ? "8580 " : "6581 ", "");
    views_append(buf, SCREEN_POPUP_TEXT, sid_chip_profile_word(id));
}

void views_number_text(char* buf, uint32_t n) {
    char d[5];
    uint32_t k = 0;
    do {
        d[k++] = (char)('0' + n % 10u);
        n /= 10u;
    } while (n && k < 4u);
    for (uint32_t i = 0; i < k; i++) {
        buf[i] = d[k - 1u - i];
    }
    buf[k] = 0;
}

void views_sound(views* v, ui_sound_id id) {
    if (v->snd) {
        ui_sound_play(v->snd, id);
    }
}

void views_preset_value(void* ctx, char* buf) {
    const views* v = ctx;
    char name[LIVE_PRESET_NAME_CHARS];
    live_preset_name(v->l, name);
    views_join(buf, MENU_VALUE_CHARS, name, "");
}

void views_preset_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    live_preset_step(v->l, steps, 0);
}

static void octave_value(void* ctx, char* buf) {
    const views* v = ctx;
    views_octave_text(buf, v->l->octave);
}

static void octave_turn(void* ctx, int32_t steps) {
    views* v = ctx;
    live_button_id b = steps > 0 ? LIVE_BUTTON_OCT_UP : LIVE_BUTTON_OCT_DOWN;
    for (int32_t n = steps > 0 ? steps : -steps; n > 0; n--) {
        live_button(v->l, b);
    }
}

static void chip_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &chip_view_view, &v->chip, SCREEN_WIPE_DOWN);
}

const view* views_shows(const views* v);

static void tracker_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    if (song_refused(v)) {
        return;
    }
    screen_push(&v->s, &tracker_view_view, &v->song, SCREEN_WIPE_DOWN);
}

static void status_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    v->status_page = 0;
    screen_push(&v->s, &views_status, v, SCREEN_WIPE_DOWN);
}

static void scene_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &scene_view_view, &v->scene, SCREEN_WIPE_DOWN);
}

static void charset_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &views_charset, v, SCREEN_WIPE_DOWN);
}

static void about_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    if (v->tune) {
        (void)song_refused(v);
        return;
    }
    about_init(&v->about, v->version);
    v->about.io = v->song.io;
    v->about.silent = (uint8_t)(v->song.io && song_io_changed(v->song.io));
    if (v->song.io) {
        song_io_about(v->song.io, 1);
    }
    screen_push(&v->s, &about_view, &v->about, SCREEN_WIPE_DOWN);
}

static void sids_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    sid_view_open(&v->sids, &v->s);
}

static void midi_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    screen_push(&v->s, &views_midi, v, SCREEN_CUT);
}

static void unstack(screen* s, const view* gone) {
    uint32_t kept = 0u;
    for (uint32_t i = 0u; i < s->depth; i++) {
        if (s->stack[i].v != gone) {
            s->stack[kept++] = s->stack[i];
        }
    }
    s->depth = (uint8_t)kept;
}

static void lab_unstack(screen* s) {
    unstack(s, &lab_view_view);
}

static void lab_show(views* v, uint32_t page) {
    lab_view_page_set(&v->lab, page);
    if (screen_top(&v->s) == &views_menu) {
        (void)screen_pop(&v->s, SCREEN_CUT);
    }
    (void)screen_push(&v->s, &lab_view_view, &v->lab, SCREEN_WIPE_DOWN);
}

static void lab_open(void* ctx, int32_t steps) {
    (void)steps;
    lab_show(ctx, LAB_PAGE_OSC);
}

static void settings_open(void* ctx, int32_t steps) {
    views* v = ctx;
    (void)steps;
    v->format_arm = 0;
    screen_push(&v->s, &views_settings_view, v, SCREEN_CUT);
}

static const menu_item MENU_ITEMS[VIEWS_MENU_ITEMS] = {
    {"CHIP", NULL, chip_open},             {"TRACKER", NULL, tracker_open},
    {"SCENE", NULL, scene_open},           {"SONG", NULL, song_open},
    {"LAB", NULL, lab_open},               {"SIDS", NULL, sids_open},
    {"PRE", views_preset_value, views_preset_turn},
    {"OCTAVE", octave_value, octave_turn},
    {"SETTINGS", NULL, settings_open},     {"STATUS", NULL, status_open},
    {"MIDI", NULL, midi_open},             {"CHARSET", NULL, charset_open},
    {"ABOUT", NULL, about_open},
};
_Static_assert(VIEWS_MENU_CHIP == 0u && VIEWS_MENU_TRACKER == 1u && VIEWS_MENU_SCENE == 2u &&
                   VIEWS_MENU_SONG == 3u && VIEWS_MENU_LAB == 4u && VIEWS_MENU_SIDS == 5u &&
                   VIEWS_MENU_PRESET == 6u && VIEWS_MENU_OCTAVE == 7u &&
                   VIEWS_MENU_SETTINGS == 8u && VIEWS_MENU_STATUS == 9u &&
                   VIEWS_MENU_MIDI == 10u && VIEWS_MENU_CHARSET == 11u &&
                   VIEWS_MENU_ABOUT == 12u,
               "the menu's items");

static void charset_compose(screen* s, void* state, grid* g) {
    (void)s;
    (void)state;
    static const char HEX[] = "0123456789ABCDEF";
    const uint8_t frame = GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLACK);
    const uint8_t label = GRID_ATTR(GRID_YELLOW, GRID_BLACK);
    grid_fill(g, 0, 0, GRID_COLS, GRID_ROWS, ' ', frame);
    grid_box(g, 0, 0, GRID_COLS, GRID_ROWS, frame, 1);
    grid_text(g, 2, 0, " CHARSET ", GRID_ATTR(GRID_WHITE, GRID_BLACK));
    for (uint32_t i = 0; i < 16u; i++) {
        grid_put(g, 7u + i, 2, (uint8_t)HEX[i], label);
        grid_put(g, 4, 4u + i, (uint8_t)HEX[i], label);
        for (uint32_t c = 0; c < 16u; c++) {
            uint32_t code = i * 16u + c;
            grid_put(g, 7u + c, 4u + i,
                     code - GRID_BIG_FIRST < GRID_BIG_PIECES || code > FONT_LAST ? ' '
                                                                                  : (uint8_t)code,
                     GRID_ATTR(GRID_WHITE, GRID_BLACK));
        }
        grid_put(g, 7u + i, 22, FONT_FULL, GRID_ATTR(i, GRID_BLACK));
        grid_put(g, 7u + i, 23, FONT_D8, GRID_ATTR(i, GRID_BLACK));
        grid_put(g, 7u + i, 26, FONT_BOX_H, GRID_ATTR(GRID_DARK_GREY, GRID_BLACK));
        uint8_t mask[8] = {0};
        for (uint32_t x = 0; x < 8u; x++) {
            uint32_t p = (i * 8u + x) % 32u;
            uint32_t y = p < 16u ? 1u + p * 5u / 15u : 6u - (p - 16u) * 5u / 15u;
            mask[y] |= (uint8_t)(0x80u >> x);
        }
        grid_overlay(g, 7u + i, 26, mask, GRID_LIGHT_GREEN);
    }
    grid_text(g, 2, 22, "COL", label);
    grid_text(g, 2, 26, "OVL", label);
}

static int charset_changed(screen* s, void* state) {
    (void)s;
    (void)state;
    return 0;
}

const view views_charset = {"CHARSET", NULL, NULL, NULL, charset_compose, charset_changed};

static void remember(views* v) {
    const live* l = v->l;
    v->model = (uint8_t)l->model_req;
    v->octave = l->octave;
    v->fx_select = l->fx_select;
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        v->algorithm[k] = l->slot[k][LIVE_SLOT_ALGORITHM];
    }
}

static void song_step(views* v) {
    const view* top = screen_top(&v->s);
    const view* next = !song_view_is(top) && v->song_last ? v->song_last : song_view_next(top);
    if (!next || next == top || song_refused(v)) {
        return;
    }
    if (song_view_is(top)) {
        screen_pop(&v->s, SCREEN_CUT);
    }
    screen_push(&v->s, next, song_state(v, next), SCREEN_WIPE_DOWN);
}

#define GLO_TAP_MS 400u
__attribute__((noinline, unused)) LIVE_COLD static void glo_tap(views* v) {
    panel* p = v->p;
    uint32_t now = v->s.now + 1u;
    if (p->glo_latch) {
        p->glo_latch = 0;
        screen_toast(&v->s, "GLO OFF", SCREEN_TOAST_MS);
    } else if (v->glo_tap_ms && now - v->glo_tap_ms <= GLO_TAP_MS) {
        p->glo_latch = 1;
        screen_toast(&v->s, "GLO LATCHED", SCREEN_TOAST_MS);
    }
    v->glo_tap_ms = now;
}

#define SEND_ASK_MS 4000u
__attribute__((noinline, unused)) LIVE_COLD static void lab_send(views* v, int edit) {
    static const char HEX[] = "0123456789ABCDEF";
    const gt2_song* song = v->song.song;
    uint32_t instr = edit ? v->song.instr : 0u, notes = 0u;
    char t[SCREEN_TOAST_CHARS + 1u] = "REPLACE I00? EDIT+REC";
    if (instr && song && v->send_ask != instr && lab_song_free(song) != instr) {
        uint32_t used = 0u;
        for (uint32_t k = 0u; k < GT2_INSTR_BYTES; k++)
            used |= song->instr[instr - 1u][k];
        if (used) {
            v->send_ask = (uint8_t)instr;
            v->send_ask_at = v->s.now;
            t[9] = HEX[instr >> 4];
            t[10] = HEX[instr & 15u];
            screen_toast(&v->s, t, SEND_ASK_MS);
            return;
        }
    }
    if (instr && v->send_ask == instr && v->s.now - v->send_ask_at > SEND_ASK_MS) {
        v->send_ask = 0u;
        lab_send(v, edit);
        return;
    }
    v->send_ask = 0u;
    uint8_t sound[LAB_SOUND_BYTES];
    char name[LIVE_PRESET_NAME_CHARS + 2u];
    live_sound_get(v->l, sound);
    if (v->l->preset == LIVE_PRESET_DICE)
        live_preset_name(v->l, name);
    else
        memcpy(name, v->l->base_name, sizeof v->l->base_name);
    const char* why = song_pages_lab_send(&v->pages, sound, name, (uint32_t)v->l->model, &instr, &notes);
    if (why) {
        screen_toast(&v->s, why, SCREEN_TOAST_MS);
        return;
    }
    char* e = t;
    for (const char* w = "SENT AS I"; *w; w++)
        *e++ = *w;
    *e++ = HEX[instr >> 4];
    *e++ = HEX[instr & 15u];
    *e++ = ' ';
    for (const char* w = name; *w && e < t + SCREEN_TOAST_CHARS; w++)
        *e++ = *w;
    *e = 0;
    screen_toast(&v->s, t, SCREEN_TOAST_MS);
    lab_view_sent(&v->lab, notes, v->s.now);
    v->song.instr = (uint8_t)instr;
    v->song.instr_from_lab = 1;
    if (v->pages.arp && v->s.depth < SCREEN_DEPTH) {
        (void)screen_push(&v->s, &song_pages_table_view, &v->pages, SCREEN_WIPE_DOWN);
    } else {
        v->pages.arp = 0u;
    }
}

__attribute__((noinline, unused)) LIVE_COLD static int lab_pull(views* v) {
    const gt2_song* song = v->song.song;
    uint32_t instr = v->song.instr;
    if (!song || !instr) {
        return 0;
    }
    uint32_t used = 0u;
    for (uint32_t k = 0u; k < GT2_INSTR_BYTES; k++)
        used |= song->instr[instr - 1u][k];
    if (!used) {
        screen_toast(&v->s, "EMPTY INSTRUMENT", SCREEN_TOAST_MS);
        return 0;
    }
    uint8_t sound[LAB_SOUND_BYTES];
    live_sound_get(v->l, sound);
    uint32_t notes = lab_song_pull(song, instr, (uint32_t)v->l->model, sound);
    char name[LAB_SOUND_NAME + 1u];
    lab_song_name(song, instr, name);
    live_sound_set_base(v->l, v->l->base, name);
    v->song.instr_from_lab = 1;
    lab_unstack(&v->s);
    lab_view_pull(&v->lab, &v->s, sound, instr, notes);
    return 1;
}

__attribute__((noinline, unused)) LIVE_COLD static void lab_lift(views* v) {
    sid_status st;
    if (!sid_view_lifted(&v->sids, &st)) {
        return;
    }
    uint8_t sound[LAB_SOUND_BYTES];
    uint32_t notes = 0u;
    live_sound_get(v->l, sound);
    int read = lab_song_pull_gti(st.lift_gti, st.lift_len, (uint32_t)v->l->model, sound, &notes);
    (void)v->sids.t->lift(v->sids.t->ctx, SID_LIFT_NO_VOICE);
    if (!read) {
        return;
    }
    lab_unstack(&v->s);
    unstack(&v->s, &views_menu);
    lab_view_lift(&v->lab, &v->s, sound, st.lift_voice, notes);
    (void)screen_push(&v->s, &lab_view_view, &v->lab, SCREEN_WIPE_DOWN);
}

int global(void* ctx, const screen_event* e) {
    views* v = ctx;
    if (e->kind != SCREEN_PRESS) {
        return 0;
    }
    if (e->id == PANEL_BTN_HOME) {
        v->format_arm = 0;
#if defined(SIDOU_CPU2)
        v->cores_armed = 0;
#endif
        v->home_down = 1;
        v->home_at = v->s.now;
        return 1;
    }
    if (e->id == PANEL_BTN_SEQ) {
        if (!song_pages_jump(&v->pages, &v->s, e))
            song_step(v);
        return 1;
    }
    if (e->id == PANEL_BTN_PLAY) {
        int edit = (int)(e->held >> PANEL_BTN_EDIT & 1u);
        if (!sid_view_play(&v->sids, edit, sid_view_is(views_shows(v)))) {
            song_view_play(&v->song, edit);
        }
        return 1;
    }
    if (e->id == PANEL_BTN_REC && views_shows(v) == &lab_view_view && !v->lab.mode) {
        lab_send(v, (int)(e->held >> PANEL_BTN_EDIT & 1u));
        return 1;
    }
    if (e->id == PANEL_BTN_REC) {
        if (views_shows(v) == &sid_view_playing) {
            sid_view_rec(&v->sids, &v->s);
            return 1;
        }
        int in_song = song_view_is(views_shows(v));
        song_view_rec(&v->song, in_song);
        if (!in_song) {
            screen_toast(&v->s, v->song.rec ? "REC ON" : "REC OFF", SCREEN_TOAST_MS);
        }
        return 1;
    }
    if (e->id == PANEL_BTN_GLO) {
        glo_tap(v);
    }
    return 0;
}

static void master_moved(void* ctx) {
    views* v = ctx;
    v->s.popup_ask = ASK_BASE | ASK_MASTER;
    v->s.popup_view = NULL;
    v->s.layers = 1;
}

void views_set_panel(views* v, panel* p) {
    p->master_hook = master_moved;
    p->master_ctx = v;
}

void views_set_sounds(views* v, ui_sound* us) {
    v->snd = us;
}

void views_set_keys(views* v, keys_route* kr) {
    chip_view_set_keys(&v->chip, kr);
}

void views_set_song(views* v, const song_transport* t) {
    if (v->pages.pending)
        return;
    song_view_set_transport(&v->song, t);
}

void views_tune(views* v, int on) {
    v->tune = (uint8_t)(on != 0);
    song_view_away(&v->song, on);
}

void views_set_tune(views* v, const sid_transport* t) {
    sid_view_set_transport(&v->sids, t);
}

static const uint8_t WATCHED[4] = {LIVE_PARAM_CUTOFF, LIVE_PARAM_RESONANCE,
                                   LIVE_PARAM_PULSE_WIDTH, LIVE_PARAM_VOLUME};

static void midi_watch(void* ctx) {
    views* v = ctx;
    for (uint32_t k = 0; k < 4u; k++) {
        uint32_t now = live_param_get(v->l, WATCHED[k]);
        if (now != v->watched[k]) {
            v->watched[k] = (uint16_t)now;
            if (!v->s.popup_ask) {
                v->s.popup_ask = (uint8_t)(ASK_PARAM | WATCHED[k]);
                v->s.popup_view = NULL;
            }
        }
    }
}

void views_popup_midi(views* v, int on) {
    for (uint32_t k = 0; k < 4u; k++) {
        v->watched[k] = (uint16_t)live_param_get(v->l, WATCHED[k]);
    }
    screen_popup_watch(&v->s, on ? midi_watch : NULL, on ? v : NULL);
}

static int read_user_sound(void* ctx, uint32_t slot, char name[LAB_SOUND_NAME + 1u],
                           uint8_t sound[LAB_SOUND_BYTES]) {
    return user_sound_read(ctx, slot, name, sound);
}

void views_route(views* v, midi_route* route) {
    v->route = route;
    if (route) {
        v->song.rows_beat = route->setup.rows_per_beat;
    }
    if (route && route->parts) {
        route->parts->user = read_user_sound;
        route->parts->user_ctx = &v->sounds;
    }
}

void views_sync(views* v, midi_sync* sync) {
    v->sync = sync;
}

void views_song_io(views* v, song_io* io) {
    v->song.io = io;
}

void views_song_loaded(views* v) {
    song_view_loaded(&v->song);
    uint8_t page = v->pages.page;
    song_pages_init(&v->pages, &v->song);
    v->pages.page = page;
    step_view_init(&v->step, &v->song);
    punch_view_init(&punch_view_state, &v->song);
    v->song.gen.pages = &v->pages;
    v->songs.built = 0;
}

void views_song_back(views* v) {
    song_view* sv = &v->song;
    uint8_t place[5] = {sv->voice, sv->instr, sv->order_pos, sv->row, sv->subtune};
    step_view* st = &v->step;
    uint8_t lanes[2 + 2u * SONG_T_VOICES];
    lanes[0] = st->page;
    lanes[1] = st->gate;
    memcpy(lanes + 2, sv->gen.root, SONG_T_VOICES);
    memcpy(lanes + 2 + SONG_T_VOICES, st->instr, sizeof st->instr);
    song_pages* pg = &v->pages;
    uint16_t order_cursor = pg->order_cursor;
    uint8_t cursors[4] = {pg->subtune, pg->instr_cursor, pg->table, pg->table_row};
    views_song_loaded(v);
    sv->voice = place[0];
    sv->instr = place[1];
    sv->order_pos = place[2];
    sv->row = place[3];
    sv->subtune = place[4];
    st->page = lanes[0];
    st->gate = lanes[1];
    memcpy(sv->gen.root, lanes + 2, SONG_T_VOICES);
    memcpy(st->instr, lanes + 2 + SONG_T_VOICES, sizeof st->instr);
    pg->order_cursor = order_cursor;
    pg->subtune = cursors[0];
    pg->instr_cursor = cursors[1];
    pg->table = cursors[2];
    pg->table_row = cursors[3];
}

void views_init(views* v, grid* g, live* l, scope* sc, const char* version) {
    v->l = l;
    v->sc = sc;
    v->version = version;
    v->home_down = 0;
    v->help.done = 0;
    v->tour = 0;
    v->user_hint = 0;
    v->dice_seen = l->dice_rolls;
    v->toast_held = 0;
    v->song_last = NULL;
    v->tune = 0;
    v->snd = NULL;
    v->set = NULL;
    v->format_arm = 0;
    v->resetting = 0;
    v->formatting = 0;
    v->menu_ask = 0;
    save_key_init(&v->key);
    menu_init(&v->home, MENU_ITEMS, VIEWS_MENU_ITEMS, VIEWS_MENU_SHOWN, v);
    v->home.scale = 2;
    v->home.sel_attr = GRID_ATTR(theme_get(l->theme)->sel_fg, theme_get(l->theme)->sel_bg);
    menu_init(&v->midi, MIDI_ITEMS, VIEWS_MIDI_ITEMS, VIEWS_MENU_SHOWN, v);
    v->midi.scale = 2;
    v->midi.lines = MIDI_LINES;
    menu_init(&v->setup, SETTINGS_ITEMS, VIEWS_SETTINGS_ITEMS, VIEWS_MENU_SHOWN, v);
    v->setup.scale = 2;
    v->setup.lines = MIDI_LINES;
    v->route = NULL;
    v->sync = NULL;
    v->radio = NULL;
#if defined(SIDOU_CPU2)
    v->cores = NULL;
    v->cores_armed = 0;
#endif
    remember(v);
    v->p = NULL;
    learn_view_init(&v->learn, l, NULL);
    chip_view_init(&v->chip, l, &v->song);
    lab_view_init(&v->lab, l, &v->sounds, &v->song, sc);
    user_profiles_init(&v->users, l, NULL);
    chip_view_set_users(&v->chip, &v->users);
    user_sounds_init(&v->sounds, l, NULL);
    live_view_init(&v->live, l, sc);
    scene_view_init(&v->scene, l, sc);
    song_view_init(&v->song, l);
    song_pages_init(&v->pages, &v->song);
    step_view_init(&v->step, &v->song);
    punch_view_init(&punch_view_state, &v->song);
    v->song.gen.pages = &v->pages;
    v->songs.built = 0;
    v->songs.scanning = 0;
    v->song.editor = &v->editor;
    song_edit_init(&v->editor, NULL);
    sid_view_init(&v->sids, &v->song);
    memset(v->tune_drawn, 0, sizeof v->tune_drawn);
    screen_init(&v->s, g, &live_view_view, &v->live);
    screen_global(&v->s, global, v);
    screen_popup_source(&v->s, &v->popup, popup_text, v);
    screen_theme_source(&v->s, &l->theme);
    if (VIEWS_POPUP_MIDI) {
        views_popup_midi(v, 1);
    }
}

void views_learn(views* v, midi_learn* ml) {
    learn_view_init(&v->learn, v->l, ml);
}

void views_radio_bind(views* v, const views_radio* radio) {
    v->radio = radio;
    v->setup.count = (uint8_t)(VIEWS_SETTINGS_ITEMS + (radio != NULL));
}

void views_settings(views* v, settings* set) {
    v->set = set;
    v->sids.st = set ? set->st : NULL;
    v->users.st = set ? set->st : NULL;
    v->scene.st = set ? set->st : NULL;
    user_profiles_load(&v->users);
}

void views_sounds(views* v, store* st) {
    v->sounds.st = st;
    user_sounds_load(&v->sounds);
}

enum {
    TOUR_OFF,
    TOUR_BOOT,
    TOUR_K1
};
#define TOUR_TOAST_MS 8000u
#define TOUR_GAP_MS 1600u

void views_boot(views* v, int full, int home_held) {
    boot_init(&v->boot, v->l, v->version, full || home_held, home_held);
    v->tour = (uint8_t)(full && !home_held ? TOUR_BOOT : TOUR_OFF);
    screen_push(&v->s, &boot_view, &v->boot, SCREEN_CUT);
}

static void boot_frame(views* v, uint32_t now_ms) {
    if (screen_top(&v->s) != &boot_view) {
        return;
    }
    switch (boot_update(&v->boot, now_ms)) {
    case BOOT_REVEAL:
        screen_reveal(&v->s, SCREEN_WIPE_DOWN);
        break;
    case BOOT_END:
        screen_pop(&v->s, SCREEN_WIPE_DOWN);
        v->s.popup_ask = 0;
        break;
    default:
        break;
    }
}

__attribute__((always_inline)) static inline const view* shown(const views* v) {
    const screen* s = &v->s;
    const view* top = screen_top(s);
    if (top == &learn_view_view && s->depth >= 2u) {
        top = s->stack[s->depth - 2u].v;
    }
    return top;
}

const view* views_shows(const views* v) {
    return shown(v);
}

__attribute__((always_inline)) static inline void knob_page(views* v, panel* p) {
    v->p = p;
    p->page = (uint8_t)(shown(v) == &chip_view_view ? chip_view_page(&v->chip) : PANEL_PAGE_LIVE);
}

int curtain(const views* v) {
    const view* top = screen_top(&v->s);
    return top == &boot_view || top == &help_view_view;
}

#define HOME_HELP_MS 600u

__attribute__((noinline, unused)) static void home_go(views* v) {
    const view* from = screen_top(&v->s);
    if (curtain(v) || from == &learn_view_view) {
        return;
    }
    if (from == &views_song_menu_view && song_menu_back(v)) {
        return;
    }
    if (from == &lab_view_view && lab_view_home(&v->lab)) {
        return;
    }
    if (from == &sid_view_browser && sid_view_home(&v->sids)) {
        return;
    }
    if (from == &sid_view_playing && sid_view_lift_cancel(&v->sids)) {
        return;
    }
    if (from == &song_pages_table_view && song_pages_arp_close(&v->pages)) {
        return;
    }
    if (screen_pop(&v->s, SCREEN_WIPE_UP) != 0) {
        screen_push(&v->s, &views_menu, v, SCREEN_WIPE_DOWN);
    }
}

static uint32_t help_card_of(const views* v, uint32_t* arg) {
    const view* top = views_shows(v);
    const live* l = v->l;
    uint32_t slot = l->fx_select % LIVE_SLOTS;
    *arg = slot | (uint32_t)l->slot[slot][LIVE_SLOT_ALGORITHM] << 8;
    if (top == &live_view_view) {
        return v->live.page ? HELP_CARD_LIVE_SLOT : HELP_CARD_LIVE;
    }
    if (top == &views_menu) {
        return HELP_CARD_MENU;
    }
    if (top == &views_midi) {
        return HELP_CARD_MIDI;
    }
    if (top == &views_settings_view) {
        return v->setup.cursor == VIEWS_SETTINGS_STORE   ? HELP_CARD_STORE
               : v->setup.cursor == VIEWS_SETTINGS_RESET ? HELP_CARD_RESET
                                                         : HELP_CARD_SETTINGS;
    }
    if (top == &views_song_menu_view) {
        uint8_t page = v->songs.page;
        return page == SONG_MENU_DIR    ? HELP_CARD_FILES
               : page == SONG_MENU_NAME ? HELP_CARD_NAME
               : page == SONG_MENU_CONFIRM ? HELP_CARD_CONFIRM
                                           : HELP_CARD_SONG;
    }
    if (top == &about_view) {
        return HELP_CARD_ABOUT;
    }
    if (top == &sid_view_browser) {
        return v->sids.asking ? HELP_CARD_UNSAVED : HELP_CARD_SIDS;
    }
    if (top == &sid_view_playing) {
        return sid_view_lift_keys(&v->sids) ? HELP_CARD_LIFT : HELP_CARD_PLAYING;
    }
    if (top == &views_charset) {
        return HELP_CARD_CHARSET;
    }
    if (top == &lab_view_view) {
        static const uint8_t LAB_HELP[LAB_PAGES] = {
            HELP_CARD_LAB_OSC, HELP_CARD_LAB_ENVELOPE, HELP_CARD_LAB_FILTER,
            HELP_CARD_LAB_FILTER_ENV, HELP_CARD_LAB_MOD, HELP_CARD_LAB_LFO, HELP_CARD_LAB_FX,
            HELP_CARD_LAB_MACRO, HELP_CARD_LAB_LAYERS, HELP_CARD_LAB_TABLE};
        return v->lab.mode ? HELP_CARD_LAB_NAME : LAB_HELP[v->lab.page % LAB_PAGES];
    }
    if (top == &chip_view_view) {
        uint32_t f = chip_view_focus(&v->chip);
        return f == CHIP_VIEW_TABS     ? HELP_CARD_CHIP_TABS
               : f == CHIP_VIEW_CHIP   ? HELP_CARD_CHIP
               : f == CHIP_VIEW_EXPERT ? HELP_CARD_EXPERT
                                       : HELP_CARD_CHIP_SLOT;
    }
    if (top == &scene_view_view) {
        return HELP_CARD_SCENE;
    }
    if (top == &punch_view_view) {
        return HELP_CARD_PUNCH;
    }
    int recording = song_view_rec_playing(&v->song);
    if (recording) {
        *arg = v->song.quant;
    }
    if (top == &step_view_view) {
        return v->step.groove ? HELP_CARD_STEP_GROOVE
               : recording    ? HELP_CARD_STEP_REC
               : v->step.held != STEP_VIEW_NONE ? HELP_CARD_STEP_HELD
                                                : HELP_CARD_STEP;
    }
    if (top == &tracker_view_view) {
        return recording ? HELP_CARD_TRACKER_REC
               : v->song.rec ? HELP_CARD_TRACKER_EDIT
                             : HELP_CARD_TRACKER;
    }
    if (top == &song_pages_order_view) {
        return HELP_CARD_ORDER;
    }
    if (top == &song_pages_instr_view) {
        return HELP_CARD_INSTR;
    }
    return top == &song_pages_table_view ? (v->pages.arp ? HELP_CARD_ARP : HELP_CARD_TABLE)
                                         : HELP_CARD_STATUS;
}

static uint16_t tour_watch(const views* v, uint32_t s) {
    const live* l = v->l;
    return s ? screen_top(&v->s) == &lab_view_view
             : (uint16_t)(l->cutoff * 129u + l->slot[l->fx_select][LIVE_SLOT_AMOUNT] * 517u);
}

static const char* const TOUR_TEXT[3] = {"TURN K1", "PRESS ENV", "HOLD HOME FOR HELP"};

static void tour_begin(views* v, uint32_t now_ms, uint32_t s) {
    v->tour = (uint8_t)(TOUR_K1 + 2u * s);
    v->tour_at = now_ms;
    v->tour_ref = tour_watch(v, s);
    screen_toast(&v->s, TOUR_TEXT[s], TOUR_TOAST_MS);
}

static int tour_text(const views* v) {
    for (uint32_t i = 0; i < 3u; i++) {
        if (strcmp(v->s.toast, TOUR_TEXT[i]) == 0) {
            return 1;
        }
    }
    return 0;
}

static void tour_clear(views* v) {
    if (v->s.toast_on && tour_text(v)) {
        screen_toast(&v->s, "", 0);
    }
}

__attribute__((noinline, unused)) static void tour_frame(views* v, uint32_t now_ms) {
    uint32_t t = v->tour;
    if (t == TOUR_BOOT) {
        if (screen_top(&v->s) != &boot_view) {
            tour_begin(v, now_ms, 0);
        }
        return;
    }
    if (screen_top(&v->s) == &help_view_view) {
        tour_clear(v);
        v->tour = TOUR_OFF;
        return;
    }
    uint32_t s = (t - TOUR_K1) >> 1;
    uint32_t waited = now_ms - v->tour_at;
    if (t & 1u) {
        if (waited >= TOUR_GAP_MS) {
            tour_begin(v, now_ms, s + 1u);
        }
        return;
    }
    int acted = s < 2u && tour_watch(v, s) != v->tour_ref;
    if (acted) {
        tour_clear(v);
    }
    if (acted || waited >= TOUR_TOAST_MS) {
        v->tour = s == 2u ? TOUR_OFF : (uint8_t)(t + 1u);
        v->tour_at = now_ms;
    }
}

__attribute__((noinline, unused)) static void user_hint(views* v) {
    if (v->l->dice_rolls == v->dice_seen) {
        return;
    }
    v->user_hint = 1;
    if (!v->l->sounds_valid) {
        screen_toast(&v->s, VIEWS_USER_HINT, VIEWS_USER_HINT_MS);
    }
}

void views_flow(views* v, uint32_t now_ms) {
    if (v->tour) {
        tour_frame(v, now_ms);
    } else if (!v->user_hint) {
        user_hint(v);
    }
    if (!v->p) {
        return;
    }
    if (v->help.done && screen_top(&v->s) == &help_view_view) {
        screen_pop(&v->s, SCREEN_BANDS);
        v->s.popup_ask = 0;
    }
    v->help.done = 0;
    if (!v->home_down) {
        return;
    }
    if (v->p->held >> PANEL_BTN_HOME & 1u) {
        if (v->home_down == 1u && now_ms - v->home_at >= HOME_HELP_MS && !curtain(v) &&
            screen_top(&v->s) != &learn_view_view) {
            uint32_t arg;
            uint32_t card = help_card_of(v, &arg);
            if (help_view_open(&v->help, &v->s, card, arg) == 0) {
                v->home_down = 2u;
            } else {
                v->home_down = 0;
                home_go(v);
            }
        }
        return;
    }
    uint8_t acts = v->home_down == 1u;
    v->home_down = 0;
    if (acts) {
        home_go(v);
    }
}

static void reset_now(views* v) {
    settings_reset(v->set);
    v->resetting = 1;
    settings_save_now(v->set, v->s.now);
}

static void format_now(views* v) {
    store* st = v->set->st;
    store_format(st);
    settings_forget(v->set);
    user_profiles_forget(&v->users);
    user_sounds_forget(&v->sounds);
    v->formatting = 1;
    if (v->set->force) {
        v->set->force(1);
    }
}

static uint32_t settings_armable(const views* v) {
    if (!v->set) {
        return ARMED_NONE;
    }
    if (v->setup.cursor == VIEWS_SETTINGS_RESET) {
        return ARMED_RESET;
    }
    const store* st = v->set->st;
    return v->setup.cursor == VIEWS_SETTINGS_STORE && st && store_state(st) == STORE_STATE_FOREIGN
               ? ARMED_FORMAT
               : ARMED_NONE;
}

static void settings_save(views* v) {
    if (v->setup.cursor == VIEWS_SETTINGS_BLUETOOTH && v->radio &&
        v->radio->mode(v->radio->ctx) == VIEWS_RADIO_KEYBOARD) {
        views_devices_open(v);
        return;
    }
    uint32_t armable = settings_armable(v);
    if (armable == ARMED_NONE) {
        return;
    }
    if (v->format_arm != armable) {
        v->format_arm = (uint8_t)armable;
        v->menu_ask = v->setup.cursor;
        screen_popup_ask(&v->s, ASK_BASE | PANEL_ENC_ALGORITHM);
        views_sound(v, UI_SOUND_CONFIRM);
        return;
    }
    v->format_arm = ARMED_NONE;
    if (armable == ARMED_RESET) {
        reset_now(v);
    } else {
        format_now(v);
    }
}

static void save(views* v, uint32_t held) {
#if defined(SIDOU_CPU2)
    if (views_status_cores_save(v)) {
        return;
    }
#endif
    if (learn_view_is_open(&v->learn)) {
        learn_view_save(&v->learn);
        return;
    }
    if (views_shows(v) == &lab_view_view) {
        lab_view_save(&v->lab, &v->s, (int)(held >> PANEL_BTN_EDIT & 1u));
        return;
    }
    if (views_shows(v) == &chip_view_view) {
        chip_view_save_result r = chip_view_save(&v->chip, v->s.now);
        if (r) {
            if (r == CHIP_VIEW_SAVE_MSG) {
                screen_toast(&v->s, v->chip.msg, SCREEN_TOAST_MS);
            }
            return;
        }
    }
    if (views_shows(v) == &views_song_menu_view) {
        song_menu_ok(v);
        return;
    }
    if (views_shows(v) == &sid_view_browser) {
        sid_view_save(&v->sids, &v->s);
        return;
    }
    if (views_shows(v) == &song_pages_table_view && v->pages.arp) {
        song_pages_arp_save(&v->pages, &v->s);
        return;
    }
    if (song_view_is(views_shows(v))) {
        if (v->song.io && v->song.io->name[0]) {
            song_io_save(v->song.io, v->song.io->name, v->s.now);
        } else {
            song_name(v, 1);
            if (v->s.depth == SCREEN_DEPTH) {
                v->songs.return_view = screen_top(&v->s);
                v->songs.direct = 2;
                screen_pop(&v->s, SCREEN_CUT);
            }
            screen_push(&v->s, &views_song_menu_view, v, SCREEN_WIPE_DOWN);
        }
        return;
    }
    if (views_shows(v) == &views_menu) {
        const menu_item* it = &v->home.items[v->home.cursor];
        if (!it->value) {
            v->menu_ask = v->home.cursor;
            it->turn(v, 1);
            views_sound(v, UI_SOUND_CONFIRM);
        }
        return;
    }
    if (views_shows(v) == &views_settings_view) {
        settings_save(v);
        return;
    }
    if (views_shows(v) == &views_devices_view) {
        views_devices_save(v);
        return;
    }
    if (v->set) {
        settings_save_now(v->set, v->s.now);
    }
}

__attribute__((noinline, unused)) static uint32_t save_buttons(views* v, uint32_t pressed,
                                                            uint32_t held) {
    uint32_t save_bit = 1u << SAVE_KEY_BUTTON;
    int ends = (pressed & save_bit) && curtain(v);
    if (ends) {
        save_key_buttons(&v->key, save_bit, held | save_bit);
        save_key_spoil(&v->key);
        screen_event e = {SCREEN_PRESS, (uint8_t)SAVE_KEY_BUTTON, 0, held};
        (void)screen_input(&v->s, &e);
    }
    if (save_key_buttons(&v->key, pressed & ~(ends ? save_bit : 0u), held) == SAVE_KEY_SAVE) {
        save(v, held);
    }
    pressed &= ~save_bit;
    if (v->key.down && (pressed & (1u << PANEL_BTN_HOME))) {
        pressed &= ~(1u << PANEL_BTN_HOME);
        learn_view_ask_clear(&v->learn);
    }
    return pressed;
}

__attribute__((noinline, unused)) LIVE_COLD static void lab_buttons(views* v, uint32_t pressed,
                                                                     uint32_t held) {
    lab_view_buttons(&v->lab, &v->s, pressed, held, screen_top(&v->s) == &lab_view_view);
}

#define LAB_BUTTONS (1u << PANEL_BTN_ENV | 1u << PANEL_BTN_LFO | 1u << PANEL_BTN_FX)
__attribute__((noinline, unused)) LIVE_COLD static void live_lab(views* v, uint32_t pressed) {
    if (screen_top(&v->s) == &song_pages_instr_view ? lab_pull(v)
                                                     : screen_top(&v->s) == &live_view_view) {
        lab_show(v, pressed >> PANEL_BTN_ENV & 1u   ? LAB_PAGE_ENVELOPE
                    : pressed >> PANEL_BTN_LFO & 1u ? LAB_PAGE_MOD
                                                    : LAB_PAGE_FX);
    }
}

static void punch_open(views* v) {
    if (screen_push(&v->s, &punch_view_view, &punch_view_state, SCREEN_WIPE_DOWN) != 0) {
        (void)screen_push_over(&v->s, &punch_view_view, &punch_view_state, SCREEN_WIPE_DOWN);
    }
}

void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held) {
    knob_page(v, p);
    if (((pressed >> PANEL_BTN_EDIT) | v->lab.edit_down) & 1u) {
        lab_buttons(v, pressed, held);
    }
    if ((pressed | v->key.down) & (1u << SAVE_KEY_BUTTON)) {
        pressed = save_buttons(v, pressed, held);
    }
    if (v->lab.lone.waiting) {
        lab_view_lone_released(&v->lab, held);
    }
    if (v->song.gen.scl_down | v->song.gen.lone.waiting) {
        if (screen_top(&v->s) == &tracker_view_view) {
            tracker_view_buttons(&v->song, &v->s, held);
        } else {
            step_view_buttons(&v->step, &v->s, held);
        }
    }
    if (v->live.arp_down) {
        live_view_buttons(&v->live, held);
    }
    uint32_t rest = pressed;
    for (uint32_t b = 0; b < 32u && pressed >> b; b++) {
        if (!((pressed >> b) & 1u)) {
            continue;
        }
        screen_event e = {SCREEN_PRESS, (uint8_t)b, 0, held};
        if (screen_input(&v->s, &e)) {
            rest &= ~(1u << b);
        }
    }
    if (rest & LAB_BUTTONS) {
        if ((rest >> PANEL_BTN_FX & 1u) && song_view_is(screen_top(&v->s))) {
            punch_open(v);
            rest &= ~(1u << PANEL_BTN_FX);
        }
        if (rest & LAB_BUTTONS) {
            live_lab(v, rest);
        }
    }
    knob_page(v, p);
    panel_buttons(p, v->l, rest, held);
}

static uint32_t panel_owned(const panel* p, uint32_t keys) {
    uint32_t owned = p->quiet;
    for (uint32_t k = 0; keys >> k; k++) {
        if ((keys >> k & 1u) && p->key_note[k] != LIVE_NO_NOTE) {
            owned |= 1u << k;
        }
    }
    return owned;
}

static void punch_keys(views* v, panel* p, uint32_t down, uint32_t pressed) {
    panel_quiet_keys(p, v->l, down | pressed);
    panel_note_keys(p, v->l, down, pressed);
    (void)panel_take_glo(p);
}

void views_note_keys(views* v, panel* p, uint32_t down, uint32_t pressed) {
    song_view* sv = &v->song;
    if (down | pressed | sv->jam_keys | v->step.down) {
        const view* top = views_shows(v);
        if (top == &punch_view_view) {
            punch_keys(v, p, down, pressed);
            return;
        }
        if (top == &help_view_view && v->s.depth >= 2u) {
            top = v->s.stack[v->s.depth - 2u].v;
        }
        int glo = (int)(((p->held >> PANEL_BTN_GLO) | p->glo_latch) & 1u);
        int take = !glo && sv->t && song_view_is(top);
        uint32_t steps = 0u, jam_down = down, jam_pressed = pressed;
        if ((top == &step_view_view || top == &tracker_view_view) &&
            sv->gen.scl_down) {
            steps = song_gen_keys(sv, down, pressed);
            jam_down = jam_pressed = 0u;
        } else if (top == &step_view_view) {
            steps = step_view_keys(&v->step, &v->s, down, pressed, !glo);
            jam_down &= ~((1u << STEP_VIEW_STEPS) - 1u);
            jam_pressed &= ~((1u << STEP_VIEW_STEPS) - 1u);
        }
        uint32_t typed = top == &lab_view_view ? lab_view_name_keys(&v->lab, pressed)
                         : top == &sid_view_playing && !glo
                             ? sid_view_lift_pick(&v->sids, &v->s, pressed)
                             : 0u;
        uint32_t owned = take ? panel_owned(p, jam_down & ~jam_pressed & ~sv->jam_keys) : 0u;
        uint32_t taken = steps | typed | song_view_keys(sv, jam_down, jam_pressed, owned, take,
                                        top == &tracker_view_view ? &v->s : NULL,
                                        top == &tracker_view_view || top == &step_view_view
                                            ? &v->s : NULL);
        if (taken) {
            panel_quiet_keys(p, v->l, taken);
        }
    }
    panel_note_keys(p, v->l, down, pressed);
    if (p->glo) {
        song_view_glo(sv, panel_take_glo(p));
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void learn_page(views* v, panel* p) {
    if (views_shows(v) == &lab_view_view) {
        p->page = (uint8_t)lab_view_panel_page(&v->lab);
    }
}

void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps) {
    if (steps == 0) {
        return;
    }
    int32_t clamped = steps > INT16_MAX ? INT16_MAX : steps < -INT16_MAX ? -INT16_MAX : steps;
    screen_event e = {SCREEN_TURN, (uint8_t)enc, (int16_t)clamped, p->held};
    knob_page(v, p);
    if (v->key.down) {
        save_key_spoil(&v->key);
        if (enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4 && !learn_view_is_open(&v->learn) &&
            !curtain(v) && v->learn.ml) {
            learn_page(v, p);
            uint32_t held = views_shows(v) == &live_view_view && v->live.page
                                ? p->held | 1u << PANEL_BTN_EDIT
                                : p->held;
            if (learn_view_open(&v->learn, &v->s, p, enc, held) != 0 && v->s.depth >= SCREEN_DEPTH) {
                screen_toast(&v->s, "NO ROOM FOR LEARN", SCREEN_TOAST_MS);
            }
            return;
        }
    }
    if (enc != PANEL_ENC_SELECT) {
        v->s.popup_ask = (uint8_t)(ASK_BASE | enc | ((e.held >> 3) & ASK_EDIT));
        v->s.popup_view = v->s.stack[v->s.depth - 1u].v;
        v->s.layers = 1;
    }
    if (!screen_input(&v->s, &e)) {
        panel_encoder(p, v->l, enc, steps);
    }
}

static void tell(views* v) {
    const live* l = v->l;
    char buf[SCREEN_TOAST_CHARS + 1u];
    char num[3];
    int changed = 0;
    int toast = 0;
    const view* top = screen_top(&v->s);
    int says = top != &live_view_view && (top != &lab_view_view || v->lab.page != LAB_PAGE_FX);
    if (l->octave != v->octave) {
        views_octave_text(num, l->octave);
        views_join(buf, sizeof buf, "OCTAVE ", num);
        changed = toast = 1;
    }
    if (l->fx_select != v->fx_select) {
        if (says) {
            views_join(buf, sizeof buf, "SLOT ", SLOT_NAMES[l->fx_select % LIVE_SLOTS]);
            toast = 1;
        }
        changed = 1;
    }
    for (uint32_t k = 0; k < LIVE_SLOTS; k++) {
        uint8_t alg = l->slot[k][LIVE_SLOT_ALGORITHM];
        if (alg != v->algorithm[k]) {
            if (says) {
                views_join(buf, sizeof buf, SLOT_NAMES[k], " ");
                views_append(buf, sizeof buf, views_algorithm_name(k, alg));
                toast = 1;
            }
            changed = 1;
        }
    }
    if ((uint8_t)l->model_req != v->model) {
        views_join(buf, sizeof buf, "CHIP ", l->model_req == SID_MODEL_8580 ? "8580" : "6581");
        screen_raster(&v->s, VIEWS_RASTER_STEPS);
        changed = toast = 1;
    }
    if (changed) {
        if (toast && !(v->s.popup_ask & 7u)) {
            screen_toast(&v->s, buf, SCREEN_TOAST_MS);
        }
        views_sound(v, UI_SOUND_CHIME);
    }
    remember(v);
}

__attribute__((noinline, unused)) static void users_frame(views* v, uint32_t now_ms) {
    user_profiles_tick(&v->users, now_ms, v->set->force);
    uint32_t slot;
    char buf[24] = "SAVED TO USER n";
    user_profile_result r = user_profiles_take(&v->users, &slot);
    buf[14] = (char)('1' + slot);
    if (r == USER_PROFILE_SAVED_GAPS) {
        views_append(buf, sizeof buf, " (GAPS)");
    }
    if (r == USER_PROFILE_SAVED || r == USER_PROFILE_SAVED_GAPS) {
        screen_toast(&v->s, buf, SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_CHUNK);
    } else if (r == USER_PROFILE_FOREIGN || r == USER_PROFILE_FAILED) {
        screen_toast(&v->s, r == USER_PROFILE_FAILED ? "STORE ERROR: NOT SAVED"
                                                     : "STORE FOREIGN: NOT SAVED",
                     SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_BUZZ);
    }
    if (user_profiles_busy(&v->users)) {
        v->l->settings_dirty |= 2u;
    }
}

__attribute__((noinline, unused)) static void sounds_frame(views* v, uint32_t now_ms) {
    user_sounds_tick(&v->sounds, now_ms, v->set->force);
    uint32_t slot = 0;
    user_sound_result r = user_sounds_take(&v->sounds, &slot);
    if (r == USER_SOUND_SAVED || r == USER_SOUND_SAVED_GAPS) {
        lab_view_kept(&v->lab, slot, r == USER_SOUND_SAVED_GAPS, &v->s);
        views_sound(v, UI_SOUND_CHUNK);
    } else if (r == USER_SOUND_FOREIGN || r == USER_SOUND_FAILED || r == USER_SOUND_FULL) {
        screen_toast(&v->s, r == USER_SOUND_FAILED  ? "STORE ERROR: NOT SAVED"
                            : r == USER_SOUND_FULL ? "STORE FULL: NOT SAVED"
                                                   : "STORE FOREIGN: NOT SAVED",
                     SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_BUZZ);
    }
    if (user_sounds_busy(&v->sounds)) {
        v->l->settings_dirty |= 2u;
    }
}

__attribute__((noinline, unused)) static void store_frame(views* v, uint32_t now_ms) {
    if (!v->set) {
        return;
    }
    settings_tick(v->set, now_ms);
    if (user_profiles_busy(&v->users)) {
        users_frame(v, now_ms);
    }
    if (user_sounds_busy(&v->sounds)) {
        sounds_frame(v, now_ms);
    }
    if (!v->set->result && !v->formatting) {
        return;
    }
    switch (settings_take_result(v->set)) {
    case SETTINGS_SAVED:
        screen_toast(&v->s, v->resetting ? "RESET AND SAVED" : "SAVED", SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_CHUNK);
        break;
    case SETTINGS_SAVED_GAPS:
        screen_toast(&v->s, v->resetting ? "RESET AND SAVED (GAPS)" : "SAVED (GAPS)",
                     SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_CHUNK);
        break;
    case SETTINGS_FOREIGN:
        screen_toast(&v->s, "STORE FOREIGN: NOT SAVED", SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_BUZZ);
        break;
    case SETTINGS_FAILED:
        screen_toast(&v->s, "STORE ERROR: NOT SAVED", SCREEN_TOAST_MS);
        views_sound(v, UI_SOUND_BUZZ);
        break;
    default: break;
    }
    v->resetting = 0;
    if (v->formatting && v->set->st && !store_busy(v->set->st)) {
        store_stats stats;
        store_stats_get(v->set->st, &stats);
        if (!stats.dirty) {
            v->formatting = 0;
            if (v->set->force) {
                v->set->force(0);
            }
            screen_toast(&v->s, "STORE FORMATTED", SCREEN_TOAST_MS);
        }
    }
    if (v->formatting) {
        v->l->settings_dirty |= 2u;
    }
}

void views_prepare(views* v) {
    if (screen_animating(&v->s)) {
        return;
    }
    const view* top = views_shows(v);
    if (top == &live_view_view) {
        live_view_prepare(&v->live);
    } else if (top == &scene_view_view) {
        scene_view_prepare(&v->scene);
    }
}

int views_idle(views* v) {
    return views_shows(v) == &scene_view_view ? scene_view_idle(&v->scene) : 0;
}

void views_song(views* v, const song_ctl* sc, const song_event* ev, uint32_t n) {
    scene_view_song(&v->scene, sc, ev, n);
}

#define SONG_BEAT_MS 120u
__attribute__((noinline, unused)) LIVE_COLD static void live_song(views* v, uint32_t now_ms) {
    live_view* lv = &v->live;
    uint32_t bpm = song_view_shown_bpm(&v->song);
    lv->bpm = bpm == SONG_VIEW_NO_BPM ? 0u : (uint16_t)(bpm > 999u ? 999u : bpm);
    uint32_t entry, row;
    song_view_position(&v->song, 0, &entry, &row);
    uint32_t rb = v->song.rows_beat ? v->song.rows_beat : 4u;
    if (row != lv->row_seen) {
        lv->row_seen = (uint8_t)row;
        if (row % rb == 0u) {
            lv->beat = row % (4u * rb) == 0u ? 2u : 1u;
            lv->beat_until = now_ms + SONG_BEAT_MS;
        }
    }
    if (lv->beat && (int32_t)(now_ms - lv->beat_until) >= 0) {
        lv->beat = 0;
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void live_frame(views* v, const view* top,
                                                                    uint32_t now_ms) {
    uint32_t ask = v->s.popup_ask;
    uint32_t enc = ask & 7u;
    int control = (ask & (ASK_BASE | ASK_PARAM)) == ASK_BASE && enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4;
    if (top == &live_view_view) {
        const panel* p = v->p;
        v->live.edit = p ? (uint8_t)(p->held >> PANEL_BTN_EDIT & 1u) : 0u;
        v->live.arp = p ? (uint8_t)(p->held >> PANEL_BTN_ARP & v->live.arp_down) : 0u;
        v->live.glo = p ? (uint8_t)(((p->held >> PANEL_BTN_GLO | p->glo_latch) & 1u) | p->glo_latch << 1)
                        : 0u;
        const song_transport* t = v->song.t;
        if (t && !v->song.want) {
            t->status(t->ctx, &v->song.st);
        }
        v->live.playing = (uint8_t)(song_view_playing(&v->song) != 0);
        if (v->live.playing) {
            live_song(v, now_ms);
        } else {
            v->live.bpm = 0;
            v->live.beat = 0;
            v->live.row_seen = 0xffu;
        }
        if (control && (!(ask & ASK_EDIT) || v->live.arp)) {
            live_view_flash(&v->live, enc - PANEL_ENC_K1 + 1u, now_ms);
            v->s.popup_ask = 0;
        } else if ((ask & (ASK_BASE | ASK_PARAM)) == ASK_BASE) {
            if (enc == PANEL_ENC_ALGORITHM) {
                live_view_flash(&v->live, v->live.page || v->live.edit ? LIVE_VIEW_HL_ALGORITHM
                                                                       : LIVE_VIEW_HL_PROFILE,
                                now_ms);
                v->s.popup_ask = 0;
            } else if (enc == PANEL_ENC_PRESETS) {
                v->s.popup_ask = 0;
            }
        }
        return;
    }
    const chip_view* cv = &v->chip;
    if (control && !cv->expert && (!(ask & ASK_EDIT) || !cv->chip)) {
        chip_view_flash(&v->chip, enc - PANEL_ENC_K1, now_ms);
        v->s.popup_ask = 0;
    } else if ((ask & (ASK_BASE | ASK_PARAM | ASK_EDIT)) == ASK_BASE && enc == PANEL_ENC_ALGORITHM) {
        v->s.popup_ask = 0;
    }
}

__attribute__((noinline, unused)) LIVE_COLD static void lab_frame(views* v, uint32_t now_ms) {
    uint32_t ask = v->s.popup_ask;
    uint32_t enc = ask & 7u;
    if ((ask & (ASK_BASE | ASK_PARAM)) != ASK_BASE || v->s.popup_view != &lab_view_view) {
        return;
    }
    if (enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4) {
        lab_view_flash(&v->lab, enc - PANEL_ENC_K1, now_ms);
        v->s.popup_ask = 0;
    } else if (enc == PANEL_ENC_ALGORITHM) {
        lab_view_flash(&v->lab, LAB_VIEW_HL_CHOICE - 1u, now_ms);
        v->s.popup_ask = 0;
    } else if (enc == PANEL_ENC_PRESETS) {
        v->s.popup_ask = 0;
    }
}

__attribute__((noinline, unused)) static void toast_hold(views* v) {
    if (screen_top(&v->s) == &help_view_view) {
        uint32_t left = screen_toast_take(&v->s);
        if (left && v->s.toast[0] && !tour_text(v)) {
            v->toast_held = (uint16_t)(left > 0xffffu ? 0xffffu : left);
        }
    } else if (v->toast_held) {
        if (v->s.toast[0]) {
            screen_toast(&v->s, v->s.toast, v->toast_held);
        }
        v->toast_held = 0;
    }
}

void views_frame(views* v, uint32_t now_ms) {
    boot_frame(v, now_ms);
    if (curtain(v)) {
        remember(v);
    } else {
        tell(v);
    }
    learn_view_frame(&v->learn, &v->s, now_ms);
    if (v->home_down | v->help.done | v->tour) {
        views_flow(v, now_ms);
    }
    if (v->l->settings_dirty) {
        store_frame(v, now_ms);
    }
    const view* top = views_shows(v);
    int in_song = song_view_is(top);
    if (in_song) {
        v->song_last = top;
    }
    song_view_frame(&v->song, in_song);
    int punching = top == &punch_view_view;
    if (punching && v->p) {
        punch_view_keys(&punch_view_state, v->p->quiet);
    }
    punch_view_frame(&punch_view_state, punching);
    if (v->pages.pending || v->pages.arp_dirty || v->pages.arp) {
        uint8_t previous = v->pages.status;
        (void)song_pages_poll(&v->pages);
        if (previous != v->pages.status && v->pages.status != SONG_PAGES_WAITING &&
            *song_pages_message(&v->pages)) {
            screen_toast(&v->s, song_pages_message(&v->pages), SCREEN_TOAST_MS);
            v->s.popup_ask = 0u;
        }
    }
    toast_hold(v);
    if (screen_top(&v->s) == &sid_view_playing) {
        lab_lift(v);
    }
    v->live.mutes = (uint8_t)song_view_mutes(&v->song);
    live_view_tap(&v->live, top == &live_view_view);
    scene_view_tap(&v->scene, top == &scene_view_view);
    if (v->sc) {
        v->sc->on = (uint8_t)(top == &live_view_view || top == &scene_view_view ||
                              top == &lab_view_view);
    }
    if (top == &live_view_view || top == &chip_view_view) {
        live_frame(v, top, now_ms);
    } else if (top == &lab_view_view) {
        lab_frame(v, now_ms);
    }
    screen_frame(&v->s, now_ms);
}

void views_update(views* v, uint32_t step, int32_t code) {
    static const char* const STEP[6] = {"", "PACKAGE", "CHECK HEAD", "LOADER", "CONFIRM",
                                        "RESTART"};
    grid* g = v->s.g;
    const uint8_t a = THEME_ATTR(theme_get(v->l->theme), value);
    widget_frame(g, 2, 18, 26, 5, a, "UPDATE");
    grid_fill(g, 3, 19, 24, 3, ' ', a);
    if (step < 9u) {
        grid_text(g, 4, 20, STEP[step < 6u ? step : 0u], a);
    } else if (code == 1) {
        grid_text(g, 4, 20, "DRY RUN OK", a);
    } else {
        grid_text(g, 4, 20, "FAILED", a);
        widget_number(g, 11, 20, 3, code, 1, a);
    }
    grid_fill(g, 2, STATUS_READY_ROW, 6, 1, ' ', a);
    screen_redraw(&v->s);
}

static int save_asked(const views* v) {
    if (learn_view_is_open(&v->learn)) {
        return v->learn.ml && v->learn.ml->state == MIDI_LEARN_CONFIRM;
    }
    const view* top = views_shows(v);
    if (top == &views_song_menu_view) {
        return v->songs.page == SONG_MENU_CONFIRM;
    }
    if (top == &sid_view_browser) {
        return v->sids.asking;
    }
    return top == &views_settings_view && v->format_arm;
}

static uint32_t beat_phase(const views* v) {
    const song_transport* t = v->song.t;
    song_status st;
    if (!t) {
        return PANEL_BEAT_NONE;
    }
    t->status(t->ctx, &st);
    uint32_t n = 0;
    while (n < SONG_T_VOICES && st.row[n] == SONG_T_NOWHERE) {
        n++;
    }
    uint32_t rb = v->song.rows_beat ? v->song.rows_beat : 4u;
    uint32_t len = ((uint32_t)st.speed[0] + st.speed[1]) * 128u;
    if (st.state != SONG_T_PLAYING || n == SONG_T_VOICES || len == 0u) {
        return PANEL_BEAT_NONE;
    }
    uint32_t in_row = ((uint32_t)(uint16_t)(st.now_call - st.row_call[n]) << 8) + st.now_frac;
    uint32_t pos = st.row[n] % rb * len + (in_row < len ? in_row : len - 1u);
    return pos * 256u / (rb * len);
}

void views_leds(views* v, uint32_t now_ms, panel_led_in* out) {
    const panel* p = v->p;
    const view* top = views_shows(v);
    int in_song = song_view_is(top);
    out->ms = now_ms;
    out->beat = (uint8_t)(p && p->playing ? beat_phase(v) : PANEL_BEAT_NONE);
    out->rec = !v->song.rec ? PANEL_REC_OFF
               : !in_song || !song_view_rec_playing(&v->song) ? PANEL_REC_ARMED
               : song_view_recording(&v->song)                ? PANEL_REC_ON
                                                              : PANEL_REC_COUNT;
    out->silent = (uint8_t)song_view_mute_mask(&v->song);
    out->solo = v->song.solo;
    int punching = top == &punch_view_view;
    out->pick = (uint8_t)(top == &sid_view_playing ? sid_view_lift_keys(&v->sids)
                          : punching                 ? punch_view_lit(&punch_view_state)
                                                     : 0u);
    out->pads = (uint8_t)(punching && punch_view_state.playing ? PUNCH_VIEW_ALL : 0u);
    out->ask = (uint8_t)save_asked(v);
    out->step_on = (uint8_t)(top == &step_view_view);
    out->step_head = PANEL_NO_STEP;
    out->step_set = out->step_rows = 0;
    if (out->step_on) {
        uint32_t set, rows, head;
        step_view_leds(&v->step, &set, &rows, &head);
        out->step_set = (uint16_t)set;
        out->step_rows = (uint16_t)rows;
        if (head != v->led_head) {
            v->led_head = (uint8_t)head;
            v->led_head_ms = now_ms;
        }
        out->step_head = head == STEP_VIEW_NONE ? PANEL_NO_STEP : (uint8_t)head;
    } else {
        v->led_head = STEP_VIEW_NONE;
    }
    out->head_ms = v->led_head_ms;
}
