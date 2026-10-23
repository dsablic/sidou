// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "song_pages.h"

#include <stddef.h>
#include <string.h>

#include "font.h"
#include "tracker_view.h"

static const char HEX[] = "0123456789ABCDEF";
static const char* const TABLE_NAMES[] = {"WAVE", "PULSE", "FILTER", "SPEED"};
static const char* const PARAM_NAMES[] = {
    "ATTACK/DECAY", "SUSTAIN/RELEASE", "WAVE PTR", "PULSE PTR", "FILTER PTR",
    "VIBRATO", "VIB DELAY", "HR/GATE", "1ST WAVE"
};
static const char* const PARAM_SHORT[] = {
    "ATK/DEC", "SUS/REL", "WAVE PTR", "PULSE PTR", "FILT PTR",
    "VIBRATO", "VIB DELAY", "HR/GATE", "1ST WAVE"
};

static uint32_t clamp(int32_t n, uint32_t lo, uint32_t hi) {
    return n < (int32_t)lo ? lo : n > (int32_t)hi ? hi : (uint32_t)n;
}

static uint8_t cycle(uint32_t n, int32_t steps, uint32_t count) {
    int32_t v = ((int32_t)n + steps) % (int32_t)count;
    return (uint8_t)(v < 0 ? v + (int32_t)count : v);
}

static void hex2(uint32_t value, char out[3]) {
    out[0] = HEX[value >> 4 & 15u];
    out[1] = HEX[value & 15u];
    out[2] = 0;
}

void song_pages_init(song_pages* p, song_view* sv) {
    memset(p, 0, sizeof *p);
    p->sv = sv;
    p->order_cursor = 1u;
    p->page = 0xffu;
}

const char* song_pages_message(const song_pages* p) {
    switch (p->status) {
    case SONG_PAGES_WAITING: return "WAITING";
    case SONG_PAGES_TIDYING: return "TIDYING";
    case SONG_PAGES_SAVING: return "SAVING";
    case SONG_PAGES_FULL: return "SONG FULL";
    case SONG_PAGES_INVALID: return "INVALID EDIT";
    case SONG_PAGES_UNAVAILABLE: return "NO SONG";
    case SONG_PAGES_BUSY: return "SONG BUSY";
    default: return "";
    }
}

static int guarded(song_pages* p) {
    const song_transport* t = p->transport;
    if (!t || !t->song || !t->edit || !t->edit_done || !t->edit_guard) {
        p->status = SONG_PAGES_UNAVAILABLE;
        return 0;
    }
    int result = t->edit_guard(t->ctx, &p->request);
    if (result != SONG_EDIT_GUARD_OK) {
        p->status = result == SONG_EDIT_GUARD_SAVING ? SONG_PAGES_SAVING :
                    result == SONG_EDIT_GUARD_BUSY ? SONG_PAGES_BUSY : SONG_PAGES_FULL;
        return 0;
    }
    return 1;
}

static gt2_err prepare(song_pages* p) {
    gt2_song* song = (gt2_song*)p->transport->song;
    const song_edit_request* r = &p->request;
    uint32_t kind = r->kind, value = r->cell[0], voice = r->cell[1];
    if (kind == SONG_PAGES_EDIT_INSTR)
        return gt2_set_instr(song, r->pattern, r->row, (uint8_t)value, &p->edit);
    if (kind == SONG_PAGES_EDIT_TABLE)
        return gt2_set_table(song, r->pattern, r->row, voice, (uint8_t)value, &p->edit);
    if (kind == SONG_PAGES_EDIT_DELETE)
        return gt2_order_delete(song, r->pattern, voice, r->row, &p->edit);
    if (kind == SONG_PAGES_EDIT_RESTART)
        return gt2_set_order(song, r->pattern, voice, r->row, (uint8_t)value, &p->edit);
    /* Publish an empty pattern first; only a later acknowledged step can name it. */
    if (value < GT2_REPEAT && !song->pattern[value]) {
        gt2_err result = gt2_order_pattern_check(song, r->pattern, voice, r->row,
                                                (uint8_t)value, kind == SONG_PAGES_EDIT_INSERT);
        if (result != GT2_OK)
            return result;
        p->auxiliary = 1u;
        return gt2_pattern_add_at(song, value, GT2_DEFAULT_ROWS, &p->edit);
    }
    if (kind == SONG_PAGES_EDIT_INSERT)
        return gt2_order_insert(song, r->pattern, voice, r->row, (uint8_t)value, &p->edit);
    return gt2_set_order(song, r->pattern, voice, r->row, (uint8_t)value, &p->edit);
}

static void arp_issue(song_pages* p);

int song_pages_poll(song_pages* p) {
    if (!p->pending)
        return 0;
    const song_transport* t = p->transport;
    if (p->sent) {
        if (!t->edit_done(t->ctx, p->ticket))
            return 0;
        p->sent = p->prepared = 0u;
        if (p->auxiliary) {
            p->auxiliary = 0u;
            return 0;
        }
        p->pending = 0u;
        p->status = SONG_PAGES_READY;
        p->revision++;
        if (p->arp_next < p->arp_end)
            arp_issue(p); /* the ARP helper's next cell */
        return 1;
    }
    if (!guarded(p)) {
        if (p->status == SONG_PAGES_SAVING && p->prepared)
            p->status = SONG_PAGES_WAITING;
        else {
            if (p->status != SONG_PAGES_SAVING) {
                if (p->prepared)
                    gt2_edit_cancel((gt2_song*)t->song, &p->edit);
                p->pending = p->prepared = p->sent = p->auxiliary = 0u;
                p->arp_next = p->arp_end;
            }
            return 0;
        }
    }
    if (!p->prepared) {
        p->auxiliary = 0u;
        p->error = prepare(p);
        if (p->error == GT2_ERR_FRAGMENTED) {
            p->status = SONG_PAGES_TIDYING;
            if (gt2_compact_step((gt2_song*)t->song, &p->edit) <= 0)
                return 0;
            p->auxiliary = 2u;
        } else if (p->error == GT2_ERR_PENDING) {
            p->status = SONG_PAGES_WAITING;
            return 0;
        } else if (p->error == GT2_ERR_SAVING) {
            p->status = SONG_PAGES_SAVING;
            return 0;
        } else if (p->error != GT2_OK) {
            p->status = p->error == GT2_ERR_FULL || p->error == GT2_ERR_SONG_FULL ?
                        SONG_PAGES_FULL : SONG_PAGES_INVALID;
            p->pending = 0u;
            p->arp_next = p->arp_end;
            return 0;
        }
        p->prepared = 1u;
    }
    if (t->edit(t->ctx, &p->edit, &p->ticket))
        p->sent = 1u;
    if (p->auxiliary != 2u)
        p->status = SONG_PAGES_WAITING;
    return 0;
}

static uint32_t entries(const song_pages* p) {
    return p->sv->song ? gt2_order_entries(p->sv->song, p->subtune, p->sv->voice) : 0u;
}

static const uint8_t* order(const song_pages* p) {
    return p->sv->song ? gt2_order_data(p->sv->song, p->subtune, p->sv->voice) : NULL;
}

static void start(song_pages* p, screen* s, uint32_t kind, uint32_t index, uint32_t row,
                  uint8_t value, uint8_t side) {
    if (p->pending) {
        screen_toast(s, "WAITING", SCREEN_TOAST_MS);
        s->popup_ask = 0u;
        return;
    }
    p->transport = p->sv->t;
    p->request = (song_edit_request){.kind = kind, .pattern = index, .row = row,
                                   .cell = {value, side, 0u, 0u}};
    if (!guarded(p)) {
        screen_toast(s, song_pages_message(p), SCREEN_TOAST_MS);
        s->popup_ask = 0u;
        return;
    }
    p->pending = 1u;
    p->prepared = p->sent = p->auxiliary = 0u;
    p->status = SONG_PAGES_WAITING;
    int structural = kind == SONG_PAGES_EDIT_INSERT || kind == SONG_PAGES_EDIT_DELETE ||
                     (kind == SONG_PAGES_EDIT_ORDER && value < GT2_REPEAT &&
                      !p->transport->song->pattern[value]);
    if (!structural)
        song_pages_poll(p);
    if (p->status != SONG_PAGES_WAITING && p->status != SONG_PAGES_READY) {
        screen_toast(s, song_pages_message(p), SCREEN_TOAST_MS);
        s->popup_ask = 0u;
    }
}

/* ORDER's cursor is the song views' entry; a move to another entry (or subtune) starts it at
 * row 0, but TRACKER's row stays while the entry does (ORDER passed through keeps it). */
static void sync_order(song_pages* p) {
    uint32_t n = entries(p);
    p->order_cursor = (uint16_t)clamp(p->order_cursor, 0u, n + 1u);
    uint8_t pos = (uint8_t)(p->order_cursor && n ?
        clamp((int32_t)p->order_cursor - 1, 0u, n - 1u) : 0u);
    if (pos != p->sv->order_pos || p->subtune != p->sv->subtune)
        p->sv->row = 0u;
    p->sv->subtune = p->subtune;
    p->sv->order_pos = pos;
}

static uint8_t order_value(uint8_t value, int32_t steps) {
    if (value < GT2_REPEAT)
        return (uint8_t)clamp((int32_t)value + steps, 0u, GT2_MAX_PATTERNS - 1u);
    if (value < GT2_TRANSDOWN) {
        uint32_t count = value & 15u ? value & 15u : 16u;
        return (uint8_t)(GT2_REPEAT | (clamp((int32_t)count + steps, 1u, 16u) & 15u));
    }
    int32_t transpose = (int32_t)value - (int32_t)GT2_TRANSUP + steps;
    return (uint8_t)(transpose < -16 ? (int32_t)GT2_TRANSDOWN : transpose > 14 ? 0xfe :
                     (int32_t)GT2_TRANSUP + transpose);
}

static uint8_t popup_byte(const song_pages* p, uint32_t kind, uint32_t index, uint32_t row,
                          uint32_t side, uint8_t current) {
    const song_edit_request* r = &p->request;
    if (p->pending && p->transport && p->transport->song == p->sv->song &&
        r->kind == kind && r->pattern == index && r->row == row && r->cell[1] == side)
        return r->cell[0];
    return current;
}

static void popup_text(char out[SCREEN_POPUP_TEXT], const char* text) {
    uint32_t n = 0u;
    while (n < SCREEN_POPUP_TEXT - 1u && text[n])
        n++;
    memcpy(out, text, n);
    out[n] = 0;
}

/* A number into popup text, with its sign when `sign` or negative. */
static void arp_num(char out[SCREEN_POPUP_TEXT], int32_t v, int sign) {
    uint32_t n = 0u, a = (uint32_t)(v < 0 ? -v : v);
    if (v < 0 || sign)
        out[n++] = v < 0 ? '-' : '+';
    if (a >= 100u)
        out[n++] = (char)('0' + a / 100u);
    if (a >= 10u)
        out[n++] = (char)('0' + a / 10u % 10u);
    out[n++] = (char)('0' + a % 10u);
    out[n] = 0;
}

/* The helper's controls' popups: the offset, the speed, the loop step (HOLD for none) and the
 * number of steps. */
static int arp_popup(const song_pages* p, uint32_t encoder, char name[SCREEN_POPUP_TEXT],
                     char value[SCREEN_POPUP_TEXT], uint32_t* fill, uint32_t* max) {
    const song_pages_arp* a = &p->arp_draft;
    const char* label;
    char text[SCREEN_POPUP_TEXT];
    if (encoder == PANEL_ENC_K1 || encoder == SONG_PAGES_ENC_K2) {
        label = "OFFSET";
        arp_num(text, a->off[p->arp_cur], 1);
        *fill = (uint32_t)(a->off[p->arp_cur] + 64);
        *max = 127u;
    } else if (encoder == PANEL_ENC_K1 + 2u) {
        label = "SPEED";
        arp_num(text, a->speed, 0);
        *fill = a->speed - 1u;
        *max = SONG_PAGES_ARP_SPEED_MAX - 1u;
    } else if (encoder == PANEL_ENC_K1 + 3u) {
        label = "LOOP";
        if (a->loop)
            arp_num(text, a->loop, 0);
        else
            popup_text(text, "HOLD");
        *fill = a->loop;
        *max = a->steps;
    } else if (encoder == PANEL_ENC_ALGORITHM) {
        label = "STEPS";
        arp_num(text, a->steps, 0);
        *fill = a->steps - 1u;
        *max = SONG_PAGES_ARP_STEPS - 1u;
    } else {
        return 0;
    }
    popup_text(name, label);
    popup_text(value, text);
    return 1;
}

int song_pages_popup(const song_pages* p, const view* owner, uint32_t encoder, int edit,
                     char name[SCREEN_POPUP_TEXT], char value[SCREEN_POPUP_TEXT],
                     uint32_t* fill, uint32_t* max) {
    (void)edit;
    if (!p || !p->sv)
        return 0;
    uint32_t page = owner == &song_pages_order_view ? SONG_VIEW_ORDER :
                    owner == &song_pages_instr_view ? SONG_VIEW_INSTR :
                    owner == &song_pages_table_view ? SONG_VIEW_TABLE : SONG_VIEW_COUNT;
    const song_view* sv = p->sv;
    const char* label;
    char text[SCREEN_POPUP_TEXT] = {0};
    uint32_t v = 0u, lo = 0u, hi = 255u;
    if (page == SONG_VIEW_ORDER) {
        if (encoder == PANEL_ENC_ALGORITHM) {
            label = p->order_cursor ? "VOICE" : "SUBTUNE";
            v = p->order_cursor ? sv->voice + 1u : p->subtune + 1u;
            lo = 1u;
            hi = p->order_cursor ? SONG_T_VOICES : sv->song ? sv->song->subtunes : 1u;
        } else {
            if (encoder != PANEL_ENC_K1 && encoder != SONG_PAGES_ENC_K2)
                return 0;
            const uint8_t* o = order(p);
            uint32_t n = entries(p);
            if (!o || !p->order_cursor || p->order_cursor > n + 1u)
                return 0;
            uint32_t pos = p->order_cursor - 1u;
            if (pos == n) {
                if (encoder != PANEL_ENC_K1)
                    return 0;
                label = "RESTART";
                v = popup_byte(p, SONG_PAGES_EDIT_RESTART, p->subtune, n, sv->voice, o[n + 1u]);
                hi = n ? n - 1u : 0u;
            } else {
                uint8_t entry = popup_byte(p, SONG_PAGES_EDIT_ORDER, p->subtune, pos, sv->voice, o[pos]);
                if (entry == GT2_LOOPSONG)
                    return 0;
                label = entry < GT2_REPEAT ? "PATTERN" : entry < GT2_TRANSDOWN ? "REPEAT" : "TRANSPOSE";
                if (encoder == SONG_PAGES_ENC_K2) {
                    popup_text(text, label);
                    label = "ORDER KIND";
                    hi = 0u;
                } else if (entry < GT2_REPEAT) {
                    v = entry;
                    hi = GT2_MAX_PATTERNS - 1u;
                } else if (entry < GT2_TRANSDOWN) {
                    v = entry & 15u ? entry & 15u : 16u;
                    lo = 1u;
                    hi = 16u;
                    if (v == 16u)
                        popup_text(text, "R16");
                    else
                        tracker_view_order_entry(entry, text);
                } else {
                    v = entry - GT2_TRANSDOWN;
                    hi = 0xfeu - GT2_TRANSDOWN;
                    tracker_view_order_entry(entry, text);
                }
            }
        }
    } else if (page == SONG_VIEW_INSTR) {
        if (sv->instr < 1u || sv->instr > GT2_MAX_INSTR)
            return 0;
        if (encoder == SONG_PAGES_ENC_PRESETS) {
            label = "INSTRUMENT";
            v = sv->instr;
            lo = 1u;
            hi = GT2_MAX_INSTR;
        } else {
            if (!sv->song || p->instr_cursor >= GT2_INSTR_BYTES ||
                (encoder != PANEL_ENC_K1 && encoder != SONG_PAGES_ENC_K2))
                return 0;
            v = popup_byte(p, SONG_PAGES_EDIT_INSTR, sv->instr, p->instr_cursor, 0u,
                           sv->song->instr[sv->instr - 1u][p->instr_cursor]);
            if (p->instr_cursor < GT2_INS_NAME) {
                label = p->instr_cursor == GT2_INS_SR ? "SUSTAIN/REL" : PARAM_NAMES[p->instr_cursor];
            } else {
                label = "NAME CHAR";
                if (v >= 32u && v <= 126u) {
                    lo = 32u;
                    hi = 126u;
                    if (v == 32u)
                        popup_text(text, "SPACE");
                    else {
                        text[0] = text[2] = '\'';
                        text[1] = (char)v;
                    }
                } else {
                    hi = 0u;
                }
            }
        }
    } else if (page == SONG_VIEW_TABLE) {
        if (p->arp)
            return arp_popup(p, encoder, name, value, fill, max);
        if (p->table >= GT2_TABLES)
            return 0;
        if (encoder == PANEL_ENC_ALGORITHM) {
            label = "TABLE";
            popup_text(text, TABLE_NAMES[p->table]);
            hi = 0u;
        } else {
            if (!sv->song || p->table_row >= GT2_TABLE_ROWS ||
                (encoder != PANEL_ENC_K1 && encoder != SONG_PAGES_ENC_K2))
                return 0;
            uint32_t side = encoder == SONG_PAGES_ENC_K2;
            label = side ? "RIGHT BYTE" : "LEFT BYTE";
            uint8_t current = side ? sv->song->rtable[p->table][p->table_row] :
                                     sv->song->ltable[p->table][p->table_row];
            v = popup_byte(p, SONG_PAGES_EDIT_TABLE, p->table, p->table_row, side, current);
        }
    } else {
        return 0;
    }
    if (!text[0])
        hex2(v, text);
    popup_text(name, label);
    popup_text(value, text);
    *fill = hi > lo ? v - lo : 0u;
    *max = hi > lo ? hi - lo : 0u;
    return 1;
}

static int order_input(screen* s, void* state, const screen_event* e) {
    song_pages* p = state;
    p->page = SONG_VIEW_ORDER;
    if (p->sent)
        (void)song_pages_poll(p);
    song_view* sv = p->sv;
    uint32_t n = entries(p);
    const uint8_t* o = order(p);
    if (e->kind == SCREEN_TURN && e->id == PANEL_ENC_SELECT) {
        p->order_cursor = (uint16_t)clamp((int32_t)p->order_cursor + e->steps, 0u, n + 1u);
        sync_order(p);
        return 1;
    }
    if (e->kind == SCREEN_TURN && e->id == PANEL_ENC_ALGORITHM) {
        if (!p->order_cursor) {
            uint32_t tunes = sv->song && sv->song->subtunes ? sv->song->subtunes : 1u;
            p->subtune = cycle(p->subtune, e->steps, tunes);
        } else {
            sv->voice = cycle(sv->voice, e->steps, SONG_T_VOICES);
        }
        sync_order(p);
        return 1;
    }
    uint32_t pos = p->order_cursor ? p->order_cursor - 1u : 0u;
    if (e->kind == SCREEN_PRESS && e->id == PANEL_BTN_ARP) {
        if (o && p->order_cursor && pos < n)
            start(p, s, SONG_PAGES_EDIT_RESTART, p->subtune, n, (uint8_t)pos, sv->voice);
        return 1;
    }
    if (e->kind != SCREEN_TURN)
        return 0;
    if (e->id == SONG_PAGES_ENC_PRESETS && (e->held >> PANEL_BTN_EDIT & 1u)) {
        if (o && p->order_cursor && e->steps) {
            uint8_t value = pos < n && o[pos] < GT2_REPEAT ? o[pos] : 0u;
            start(p, s, e->steps > 0 ? SONG_PAGES_EDIT_INSERT : SONG_PAGES_EDIT_DELETE,
                  p->subtune, pos, value, sv->voice);
        }
        return 1;
    }
    if (e->id != PANEL_ENC_K1 && e->id != SONG_PAGES_ENC_K2)
        return 0;
    if (!o || !p->order_cursor || !e->steps)
        return 1;
    if (pos >= n) {
        if (e->id == PANEL_ENC_K1)
            start(p, s, SONG_PAGES_EDIT_RESTART, p->subtune, n,
                  (uint8_t)clamp((int32_t)o[n + 1u] + e->steps, 0u, n ? n - 1u : 0u), sv->voice);
    } else {
        uint8_t value = order_value(o[pos], e->steps);
        if (e->id == SONG_PAGES_ENC_K2) {
            uint32_t kind = o[pos] < GT2_REPEAT ? 0u : o[pos] < GT2_TRANSDOWN ? 1u : 2u;
            static const uint8_t DEFAULTS[] = {0u, GT2_REPEAT | 2u, GT2_TRANSUP};
            value = DEFAULTS[cycle(kind, e->steps, 3u)];
        }
        start(p, s, SONG_PAGES_EDIT_ORDER, p->subtune, pos, value, sv->voice);
    }
    return 1;
}

static int instr_input(screen* s, void* state, const screen_event* e) {
    song_pages* p = state;
    p->page = SONG_VIEW_INSTR;
    if (p->sent)
        (void)song_pages_poll(p);
    song_view* sv = p->sv;
    if (e->kind != SCREEN_TURN)
        return 0;
    if (e->id == PANEL_ENC_SELECT) {
        p->instr_cursor = (uint8_t)clamp((int32_t)p->instr_cursor + e->steps, 0u, GT2_INSTR_BYTES - 1u);
        return 1;
    }
    if (e->id == SONG_PAGES_ENC_PRESETS) {
        sv->instr = (uint8_t)clamp((int32_t)sv->instr + e->steps, 1u, GT2_MAX_INSTR);
        return 1;
    }
    if (e->id != PANEL_ENC_K1 && e->id != SONG_PAGES_ENC_K2)
        return 0;
    if (sv->song && e->steps) {
        uint32_t old = sv->song->instr[sv->instr - 1u][p->instr_cursor];
        uint32_t lo = p->instr_cursor >= GT2_INS_NAME ? 32u : 0u;
        uint32_t hi = lo ? 126u : 255u;
        int32_t value = (int32_t)(old < lo ? lo : old) + e->steps * (e->id == PANEL_ENC_K1 ? 16 : 1);
        start(p, s, SONG_PAGES_EDIT_INSTR, sv->instr, p->instr_cursor,
              (uint8_t)clamp(value, lo, hi), 0u);
    }
    return 1;
}

static int arp_input(song_pages* p, const screen_event* e);

static int table_input(screen* s, void* state, const screen_event* e) {
    song_pages* p = state;
    p->page = SONG_VIEW_TABLE;
    if (p->sent)
        (void)song_pages_poll(p);
    if (e->kind == SCREEN_PRESS && e->id == PANEL_BTN_ARP && !p->arp) {
        (void)song_pages_arp_open(p, s);
        return 1;
    }
    if (e->kind != SCREEN_TURN)
        return 0;
    if (p->arp)
        return arp_input(p, e);
    if (e->id == PANEL_ENC_SELECT) {
        p->table_row = (uint8_t)clamp((int32_t)p->table_row + e->steps, 0u, GT2_TABLE_ROWS - 1u);
        return 1;
    }
    if (e->id == PANEL_ENC_ALGORITHM) {
        p->table = cycle(p->table, e->steps, GT2_TABLES);
        return 1;
    }
    if (e->id != PANEL_ENC_K1 && e->id != SONG_PAGES_ENC_K2)
        return 0;
    if (p->sv->song && e->steps) {
        uint32_t side = e->id == SONG_PAGES_ENC_K2;
        uint8_t old = side ? p->sv->song->rtable[p->table][p->table_row] :
                             p->sv->song->ltable[p->table][p->table_row];
        start(p, s, SONG_PAGES_EDIT_TABLE, p->table, p->table_row,
              (uint8_t)clamp((int32_t)old + e->steps, 0u, 255u), (uint8_t)side);
    }
    return 1;
}

int song_pages_jump(song_pages* p, screen* s, const screen_event* e) {
    if (screen_top(s) != &song_pages_instr_view || e->kind != SCREEN_PRESS ||
        e->id != PANEL_BTN_SEQ || !(e->held >> PANEL_BTN_EDIT & 1u))
        return 0;
    uint32_t field = p->instr_cursor;
    if (field < GT2_INS_WAVE || field > GT2_INS_VIBPARAM) {
        screen_toast(s, "SELECT A PTR", SCREEN_TOAST_MS);
        return 1;
    }
    if (!p->sv->song)
        return 1;
    uint32_t ptr = p->sv->song->instr[p->sv->instr - 1u][field];
    p->table = (uint8_t)(field - GT2_INS_WAVE);
    p->table_row = (uint8_t)(ptr ? ptr - 1u : 0u);
    if (screen_pop(s, SCREEN_CUT) == 0)
        (void)screen_push(s, &song_pages_table_view, p, SCREEN_WIPE_DOWN);
    return 1;
}

void song_pages_table_meaning(uint32_t table, uint8_t left, uint8_t right, char out[31]) {
    static const char NOTES[12][3] = {"C-", "C#", "D-", "D#", "E-", "F-",
                                     "F#", "G-", "G#", "A-", "A#", "B-"};
    const char* what;
    if (table != GT2_STBL && left == 0xffu)
        what = right ? "JUMP " : "STOP ";
    else if (table == GT2_WTBL)
        what = left <= 0x0fu ? "DELAY " : left >= 0xf0u ? "COMMAND " : "WAVE ";
    else if (table == GT2_PTBL)
        what = left & 0x80u ? "SET PULSE " : "PULSE SLIDE ";
    else if (table == GT2_FTBL)
        what = !left ? "CUTOFF " : left & 0x80u ? "FILTER CTRL " : "FILTER SLIDE ";
    else
        what = left & 0x80u ? "CALCULATED SPEED " : "SPEED ";
    uint32_t n = 0u;
    while (*what && n < 24u)
        out[n++] = *what++;
    if (table != GT2_STBL && left == 0xffu) {
        hex2(right, out + n);
        return;
    }
    hex2(left, out + n);
    n += 2u;
    out[n++] = ' ';
    if (table == GT2_WTBL && left >= 0x10u && left < 0xf0u) {
        if (right > 0x80u && (right & 0x7fu) < 96u) {
            uint32_t note = right & 0x7fu;
            out[n++] = NOTES[note % 12u][0];
            out[n++] = NOTES[note % 12u][1];
            out[n++] = (char)('0' + note / 12u);
            out[n] = 0;
            return;
        }
        if (right == 0x80u) {
            memcpy(out + n, "KEEP", 5u);
            return;
        }
        memcpy(out + n, right > 0x80u ? "ABS " : "REL ", 4u);
        n += 4u;
        if (right > 0x80u)
            right &= 0x7fu;
    }
    hex2(right, out + n);
}

/* ---- the ARP helper ---- */

#define ARP_JUMP 0xffu

static uint8_t arp_rel(int32_t off) {
    return (uint8_t)(off & 0x7f);
}

/* A relative note's offset: 00..3F up, 40..7F down (the player adds it to the note, modulo 128). */
static int8_t arp_off(uint8_t right) {
    return (int8_t)(right < 0x40u ? (int32_t)right : (int32_t)right - 0x80);
}

void song_pages_arp_default(song_pages_arp* a) {
    static const int8_t MAJOR[4] = {0, 4, 7, 12};
    memset(a, 0, sizeof *a);
    memcpy(a->off, MAJOR, sizeof MAJOR);
    a->steps = 4u;
    a->speed = 2u;
    a->loop = 1u;
    a->wave = SONG_PAGES_ARP_WAVE;
}

uint32_t song_pages_arp_encode(const song_pages_arp* a, uint32_t first,
                               uint8_t left[SONG_PAGES_ARP_ROWS], uint8_t right[SONG_PAGES_ARP_ROWS]) {
    uint32_t n = a->steps, k = n;
    uint8_t delay = (uint8_t)(a->speed > 1u ? a->speed - 1u : 0u);
    left[0] = a->wave;
    for (uint32_t i = 0u; i < n; i++) {
        left[i] = i && delay ? delay : a->wave;
        right[i] = arp_rel(a->off[i]);
    }
    if (n < 2u || a->loop == 0u || a->loop > n) {
        left[k] = ARP_JUMP;
        right[k] = 0u; /* hold the last note */
    } else if (delay && a->loop == 1u) {
        left[k] = delay; /* step 1 again, after the last one's time */
        right[k++] = right[0];
        left[k] = ARP_JUMP;
        right[k] = (uint8_t)(first + 1u);
    } else {
        left[k] = ARP_JUMP;
        right[k] = (uint8_t)(first + a->loop - 1u);
    }
    return k + 1u;
}

int song_pages_arp_decode(const uint8_t* left, const uint8_t* right, uint32_t first, uint32_t avail,
                          song_pages_arp* a, uint32_t* rows) {
    uint8_t wave = left[0];
    if (avail < 2u || wave < 0x10u || wave >= 0xe0u || right[0] >= 0x80u)
        return 0;
    int8_t off[SONG_PAGES_ARP_STEPS + 1u];
    uint32_t n = 1u, delayed = 0u, delay = 0u;
    off[0] = arp_off(right[0]);
    for (;; n++) {
        if (n >= avail)
            return 0;
        if (left[n] == ARP_JUMP)
            break;
        if (n > SONG_PAGES_ARP_STEPS || right[n] >= 0x80u)
            return 0;
        if (n == 1u) {
            delayed = left[1] < 0x10u;
            delay = left[1];
        }
        if (left[n] != (delayed ? delay : wave))
            return 0;
        off[n] = arp_off(right[n]);
    }
    uint32_t target = right[n], loop = 0u, used = n + 1u;
    if (target) {
        if (target < first || target - first >= n || (delayed && target == first))
            return 0;
        loop = target - first + 1u;
        if (delayed && loop == 2u && n >= 3u && off[n - 1u] == off[0]) {
            n--; /* the row that loops step 1 back */
            loop = 1u;
        }
    }
    if (n > SONG_PAGES_ARP_STEPS)
        return 0;
    memset(a, 0, sizeof *a);
    memcpy(a->off, off, n);
    a->steps = (uint8_t)n;
    a->speed = (uint8_t)(delayed ? delay + 1u : 1u);
    a->loop = (uint8_t)loop;
    a->wave = wave;
    *rows = used;
    return 1;
}

/* The next byte of the helper's rows that differs from the song's, as a table write; the writes
 * go one at a time, each from the poll that saw the last acknowledged (an edit is pending until
 * then). A write the guard refuses ends them: the status says why. */
static void arp_issue(song_pages* p) {
    const gt2_song* song = p->sv->song;
    while (song && p->arp_next < p->arp_end) {
        uint32_t k = p->arp_next++, row = p->arp_start + k / 2u, side = k & 1u;
        uint8_t value = p->arp_rows[k];
        if ((side ? song->rtable[GT2_WTBL][row] : song->ltable[GT2_WTBL][row]) == value)
            continue;
        p->request = (song_edit_request){.kind = SONG_PAGES_EDIT_TABLE, .pattern = GT2_WTBL,
                                       .row = row, .cell = {value, (uint8_t)side, 0u, 0u}};
        if (!guarded(p))
            break;
        p->pending = 1u;
        p->prepared = p->sent = p->auxiliary = 0u;
        p->status = SONG_PAGES_WAITING;
        return;
    }
    p->arp_next = p->arp_end;
}

int song_pages_arp_open(song_pages* p, screen* s) {
    const gt2_song* song = p->sv->song;
    const char* why = !song ? "NO SONG" : p->table != GT2_WTBL ? "WAVE TABLE ONLY" : NULL;
    uint32_t row = p->table_row, used = 0u;
    if (!why) {
        const uint8_t* l = song->ltable[GT2_WTBL] + row;
        const uint8_t* r = song->rtable[GT2_WTBL] + row;
        if (!l[0] && !r[0])
            song_pages_arp_default(&p->arp_draft); /* a blank row: a new one */
        else if (!song_pages_arp_decode(l, r, row + 1u, GT2_TABLE_ROWS - row, &p->arp_draft, &used))
            why = "NOT AN ARP";
    }
    if (why) {
        screen_toast(s, why, SCREEN_TOAST_MS);
        return 0;
    }
    p->arp = 1u;
    p->arp_cur = 0u;
    p->arp_start = (uint8_t)row;
    p->arp_len = (uint8_t)used;
    return 1;
}

int song_pages_arp_close(song_pages* p) {
    int open = p->arp;
    p->arp = 0u;
    return open;
}

void song_pages_arp_save(song_pages* p, screen* s) {
    const gt2_song* song = p->sv->song;
    uint8_t left[SONG_PAGES_ARP_ROWS], right[SONG_PAGES_ARP_ROWS];
    uint32_t n = song_pages_arp_encode(&p->arp_draft, p->arp_start + 1u, left, right);
    const char* why = !song ? "NO SONG" : p->pending ? "WAITING" :
                      p->arp_start + n > GT2_TABLE_ROWS ? "TABLE FULL" : NULL;
    for (uint32_t i = p->arp_len; !why && i < n; i++)
        if (song->ltable[GT2_WTBL][p->arp_start + i] | song->rtable[GT2_WTBL][p->arp_start + i])
            why = "ROWS IN USE"; /* past the rows it read: another program's */
    if (why) {
        screen_toast(s, why, SCREEN_TOAST_MS);
        return;
    }
    uint32_t end = n > p->arp_len ? n : p->arp_len; /* rows it no longer needs go blank */
    for (uint32_t i = 0u; i < end; i++) {
        p->arp_rows[2u * i] = i < n ? left[i] : 0u;
        p->arp_rows[2u * i + 1u] = i < n ? right[i] : 0u;
    }
    p->transport = p->sv->t;
    p->arp_next = 0u;
    p->arp_end = (uint8_t)(2u * end);
    p->status = SONG_PAGES_READY;
    arp_issue(p);
    song_pages_poll(p);
    if (p->status != SONG_PAGES_WAITING && p->status != SONG_PAGES_READY) {
        screen_toast(s, song_pages_message(p), SCREEN_TOAST_MS);
        return; /* the helper stays open */
    }
    p->arp = 0u;
    screen_toast(s, "ARP SAVED", SCREEN_TOAST_MS);
}

static int arp_input(song_pages* p, const screen_event* e) {
    song_pages_arp* a = &p->arp_draft;
    int32_t steps = e->steps;
    if (e->id == PANEL_ENC_SELECT) {
        p->arp_cur = (uint8_t)clamp((int32_t)p->arp_cur + steps, 0u, a->steps - 1u);
    } else if (e->id == PANEL_ENC_ALGORITHM) {
        uint32_t old = a->steps;
        a->steps = (uint8_t)clamp((int32_t)old + steps, 1u, SONG_PAGES_ARP_STEPS);
        for (uint32_t i = old; i < a->steps; i++)
            a->off[i] = a->off[old - 1u];
        a->loop = a->loop > a->steps ? a->steps : a->loop;
        p->arp_cur = p->arp_cur >= a->steps ? (uint8_t)(a->steps - 1u) : p->arp_cur;
    } else if (e->id == PANEL_ENC_K1 || e->id == SONG_PAGES_ENC_K2) {
        int32_t v = (int32_t)a->off[p->arp_cur] + steps * (e->id == PANEL_ENC_K1 ? 12 : 1);
        a->off[p->arp_cur] = (int8_t)(v < -64 ? -64 : v > 63 ? 63 : v);
    } else if (e->id == PANEL_ENC_K1 + 2u) {
        a->speed = (uint8_t)clamp((int32_t)a->speed + steps, 1u, SONG_PAGES_ARP_SPEED_MAX);
    } else if (e->id == PANEL_ENC_K1 + 3u) {
        a->loop = (uint8_t)clamp((int32_t)a->loop + steps, 0u, a->steps);
    } else {
        return 0;
    }
    return 1;
}

__attribute__((noinline, unused)) static void small(song_view_canvas* c, uint32_t col, uint32_t row,
                                                 const char* text, uint8_t attr) {
    if (col >= GRID_COLS)
        return;
    uint32_t n = 0u;
    while (n < GRID_COLS - col && text[n])
        n++;
    memcpy(c->ch[row] + col, text, n);
    memset(c->attr[row] + col, attr, n);
}

static void big(song_view_canvas* c, uint32_t col, uint32_t row, const char* text, uint8_t attr) {
    (void)grid_canvas_text_big(c, col, row, 2u, text, attr);
}

static void blank(song_view_canvas* c, uint32_t row, uint32_t rows, uint8_t attr) {
    for (uint32_t r = row; r < row + rows; r++) {
        memset(c->ch[r], ' ', GRID_COLS);
        memset(c->attr[r], attr, GRID_COLS);
    }
}

static uint32_t window_top(const song_pages* p) {
    if (p->page == SONG_VIEW_INSTR)
        return 0u;
    uint32_t row = p->page == SONG_VIEW_ORDER ? (p->order_cursor ? p->order_cursor - 1u : 0u) : p->table_row;
    return row / SONG_PAGES_ROWS * SONG_PAGES_ROWS;
}

static void build_head(song_pages* p) {
    song_view* sv = p->sv;
    song_view_canvas* c = &sv->cv;
    const theme* th = theme_get(sv->l->theme);
    uint8_t base = THEME_ATTR(th, value);
    blank(c, 0u, 3u, base);
    big(c, 0u, 0u, p->page == SONG_VIEW_ORDER ? "ORDER" : p->page == SONG_VIEW_INSTR ? "INSTR" :
        TABLE_NAMES[p->table], base);
    char number[3];
    if (p->page == SONG_VIEW_ORDER) {
        char vn[3] = {'V', (char)('1' + sv->voice), 0};
        big(c, 12u, 0u, vn, GRID_ATTR(song_view_mute_mask(sv) >> sv->voice & 1u ?
                                    th->dim : th->voice[sv->voice], th->bg));
        small(c, 0u, 2u, "SUBTUNE", THEME_ATTR(th, hint));
        hex2(p->subtune + 1u, number);
        small(c, 8u, 2u, number, GRID_ATTR(th->value, !p->order_cursor ? th->cursor : th->bg));
        uint32_t col = SONG_PAGES_OTHER_COL;
        for (uint32_t v = 0u; v < SONG_T_VOICES; v++) {
            if (v == sv->voice)
                continue;
            char tag[3] = {'V', (char)('1' + v), 0};
            small(c, col, 2u, tag, GRID_ATTR(song_view_mute_mask(sv) >> v & 1u ? th->dim : th->voice_text[v], th->bg));
            col += SONG_PAGES_OTHER_STEP;
        }
    } else if (p->page == SONG_VIEW_INSTR) {
        hex2(sv->instr, number);
        big(c, 12u, 0u, number, GRID_ATTR(th->voice[sv->voice], th->bg));
        small(c, 0u, 2u, "PRESETS: INSTRUMENT", THEME_ATTR(th, hint));
    } else {
        /* which knob turns which column, at 2x beside the name (the columns' colours) */
        if (p->arp)
            big(c, 0u, 0u, "ARP   ", base); /* over the table's name */
        big(c, 12u, 0u, "K1", GRID_ATTR(th->knob_text[0], th->bg));
        big(c, 17u, 0u, "K2", GRID_ATTR(th->knob_text[1], th->bg));
    }
    c->ch[0][22] = song_view_playing(sv) ? FONT_PLAY : FONT_SQUARE;
    c->attr[0][22] = base;
    uint32_t bpm = song_view_shown_bpm(sv);
    char tempo[5] = " ---";
    if (bpm != SONG_VIEW_NO_BPM) {
        bpm = bpm > 9999u ? 9999u : bpm;
        for (int i = 3; i >= 0; i--) {
            tempo[i] = (char)(bpm || i == 3 ? '0' + bpm % 10u : ' ');
            bpm /= 10u;
        }
    }
    small(c, 26u, 0u, tempo, THEME_ATTR(th, label));
    small(c, 26u, 1u, "BPM", THEME_ATTR(th, hint));
    if (sv->rec)
        small(c, 22u, 1u, "REC", THEME_ATTR(th, alert));
    if (sv->solo >> sv->voice & 1u)
        small(c, 18u, 1u, "SOL", base);
    else if (sv->mute >> sv->voice & 1u)
        small(c, 18u, 1u, "MUT", THEME_ATTR(th, alert));
    sv->dirty |= 7u;
}

/* The helper's page, in the table's rows: the number of steps, the speed and the loop step at 2x
 * with the knob that turns each, the steps' offsets four to a line (the step the cursor is on in
 * reverse, the loop step in yellow), and the waveform that is kept. */
static void arp_row(song_pages* p, uint32_t j) {
    song_view* sv = p->sv;
    song_view_canvas* c = &sv->cv;
    const theme* th = theme_get(sv->l->theme);
    const song_pages_arp* a = &p->arp_draft;
    uint32_t r = SONG_PAGES_TOP + 2u * j;
    char num[SCREEN_POPUP_TEXT];
    blank(c, r, 2u, THEME_ATTR(th, value));
    if (j < 3u) {
        static const char* const LABEL[3] = {"STEPS", "SPEED", "LOOP"};
        static const uint8_t KNOB[3] = {0u, 2u, 3u};
        big(c, 0u, r, LABEL[j], THEME_ATTR(th, label));
        if (j == 2u && !a->loop)
            memcpy(num, "HOLD", 5u);
        else
            arp_num(num, j == 0u ? a->steps : j == 1u ? a->speed : a->loop, 0);
        big(c, 12u, r, num, GRID_ATTR(th->knob[KNOB[j]], th->bg));
    } else if (j >= 4u && j < 6u) {
        for (uint32_t k = (j - 4u) * 4u; k < a->steps && k < (j - 3u) * 4u; k++) {
            arp_num(num, a->off[k], 1);
            uint8_t attr = k == p->arp_cur ? GRID_ATTR(th->bg, th->hot)
                           : a->loop && k == a->loop - 1u ? GRID_ATTR(th->hot, th->bg)
                                                          : THEME_ATTR(th, value);
            big(c, k % 4u * 7u, r, num, attr);
        }
    } else if (j == 7u) {
        memcpy(num, "WAVE ", 5u);
        hex2(a->wave, num + 5u);
        small(c, 0u, r, num, THEME_ATTR(th, hint));
    }
}

static void build_row(song_pages* p, uint32_t j) {
    if (p->arp && p->page == SONG_VIEW_TABLE) {
        arp_row(p, j);
        return;
    }
    song_view* sv = p->sv;
    song_view_canvas* c = &sv->cv;
    const gt2_song* song = sv->song;
    const theme* th = theme_get(sv->l->theme);
    uint32_t r = SONG_PAGES_TOP + 2u * j, u = window_top(p) + j;
    int on = p->page == SONG_VIEW_INSTR ? j < GT2_INS_NAME && j == p->instr_cursor :
             p->page == SONG_VIEW_ORDER ? p->order_cursor && u + 1u == p->order_cursor : u == p->table_row;
    uint8_t bg = on ? th->cursor : th->bg;
    uint8_t base = GRID_ATTR(th->value, bg);
    blank(c, r, 2u, base);
    if (!song)
        return;
    char num[3], text[7];
    if (p->page == SONG_VIEW_ORDER) {
        uint32_t n = entries(p);
        const uint8_t* o = order(p);
        if (!o || u > n)
            return;
        hex2(u, num);
        big(c, 0u, r, num, GRID_ATTR(on ? th->hot : th->label, bg));
        if (u == n) {
            memcpy(text, "RST ", 4u);
            hex2(o[n + 1u], text + 4u);
        } else {
            tracker_view_order_entry(o[u], text);
        }
        big(c, 5u, r, text, on ? GRID_ATTR(th->bg, th->hot) : base);
        /* The other two voices' entries at the same position, at 2x; what kind the entry is
         * (a pattern, a repeat, a transpose) is in the line under the rows. */
        uint32_t col = SONG_PAGES_OTHER_COL;
        for (uint32_t v = 0u; v < SONG_T_VOICES; v++) {
            if (v == sv->voice)
                continue;
            const uint8_t* other = gt2_order_data(song, p->subtune, v);
            uint32_t count = gt2_order_entries(song, p->subtune, v);
            if (other && u <= count) {
                if (u == count)
                    memcpy(num, "RS", 3u);
                else
                    hex2(other[u], num);
                big(c, col, r, num, GRID_ATTR(th->voice_text[v], bg));
            }
            col += SONG_PAGES_OTHER_STEP;
        }
    } else if (p->page == SONG_VIEW_INSTR) {
        if (j < GT2_INS_NAME) {
            big(c, 0u, r, PARAM_SHORT[j], GRID_ATTR(on ? th->hot : th->label, bg));
            hex2(song->instr[sv->instr - 1u][j], num);
            big(c, 22u, r, num, on ? GRID_ATTR(th->bg, th->hot) : base);
        } else if (j < GT2_INS_NAME + 2u) {
            char name[9];
            uint32_t start = (j - GT2_INS_NAME) * 8u;
            for (uint32_t k = 0u; k < 8u; k++) {
                uint8_t ch = song->instr[sv->instr - 1u][GT2_INS_NAME + start + k];
                name[k] = ch >= 32u && ch <= 126u ? (char)ch : ' ';
            }
            name[8] = 0;
            big(c, 0u, r, name, THEME_ATTR(th, value));
            if (p->instr_cursor >= GT2_INS_NAME) {
                uint32_t index = p->instr_cursor - GT2_INS_NAME;
                if (index / 8u == j - GT2_INS_NAME) {
                    char one[2] = {name[index % 8u], 0};
                    big(c, index % 8u * 2u, r, one, GRID_ATTR(th->bg, th->hot));
                }
            }
        }
    } else if (u < GT2_TABLE_ROWS) {
        hex2(u + 1u, num);
        big(c, 0u, r, num, GRID_ATTR(on ? th->hot : th->label, bg));
        hex2(song->ltable[p->table][u], num);
        big(c, 5u, r, num, on ? GRID_ATTR(th->bg, th->knob[0]) : base);
        hex2(song->rtable[p->table][u], num);
        big(c, 12u, r, num, on ? GRID_ATTR(th->bg, th->knob[1]) : GRID_ATTR(th->voice[sv->voice], bg));
    }
}

/* What the orderlist entry under the cursor is, in words: PATTERN 04, REPEAT 3, TRANSPOSE +2,
 * RESTART AT 00 for the end. */
static void order_kind(const song_pages* p, char out[24]) {
    uint32_t u = p->order_cursor - 1u, n = entries(p);
    const uint8_t* o = order(p);
    const char* word = "";
    char value[4] = {0};
    if (!o || u > n) {
        out[0] = 0;
        return;
    }
    if (u == n) {
        word = "RESTART AT ";
        hex2(o[n + 1u], value);
    } else if (o[u] < GT2_REPEAT) {
        word = "PATTERN ";
        hex2(o[u], value);
    } else if (o[u] < GT2_TRANSDOWN) {
        word = "REPEAT ";
        uint32_t times = o[u] & 15u ? o[u] & 15u : 16u;
        uint32_t k = 0u;
        if (times >= 10u)
            value[k++] = '1';
        value[k++] = (char)('0' + times % 10u);
    } else {
        word = "TRANSPOSE ";
        char e[4];
        tracker_view_order_entry(o[u], e);
        memcpy(value, e, 4u);
    }
    uint32_t k = 0u;
    while (*word)
        out[k++] = *word++;
    for (uint32_t i = 0u; value[i]; i++)
        out[k++] = value[i];
    out[k] = 0;
}

static void build_footer(song_pages* p) {
    song_view* sv = p->sv;
    const theme* th = theme_get(sv->l->theme);
    /* What the controls do is the help card's (app/help_view.h): row 27 stays blank for the toast. */
    blank(&sv->cv, 27u, 3u, THEME_ATTR(th, value));
    char meaning[31];
    const char* say = song_pages_message(p);
    char kind[24];
    if (!*say && p->page == SONG_VIEW_ORDER && sv->song && p->order_cursor) {
        order_kind(p, kind);
        say = kind;
    }
    if (!*say && p->page == SONG_VIEW_INSTR)
        say = p->instr_cursor < GT2_INS_NAME ? PARAM_NAMES[p->instr_cursor] : "INSTRUMENT NAME";
    if (!*say && p->arp && p->page == SONG_VIEW_TABLE) {
        memcpy(meaning, "SAVE WRITES FROM ROW ", 21u);
        hex2(p->arp_start + 1u, meaning + 21u);
        say = meaning;
    }
    if (!*say && p->page == SONG_VIEW_TABLE && sv->song) {
        song_pages_table_meaning(p->table, sv->song->ltable[p->table][p->table_row],
                                sv->song->rtable[p->table][p->table_row], meaning);
        say = meaning;
    }
    /* 2x on rows 28-29 when it fits 15 cells; the long meanings stay 1x on the last row. */
    uint32_t n = 0u;
    while (say[n])
        n++;
    if (n <= GRID_COLS / 2u)
        big(&sv->cv, 0u, 28u, say, THEME_ATTR(th, hot));
    else
        small(&sv->cv, 0u, 29u, say, THEME_ATTR(th, hot));
    sv->dirty |= 7u << 27u;
}

static void look(const song_pages* p, uint8_t out[SONG_PAGES_LOOK]) {
    const song_view* sv = p->sv;
    memset(out, 0, SONG_PAGES_LOOK);
    out[0] = p->page;
    out[1] = p->subtune;
    out[2] = (uint8_t)p->order_cursor;
    out[3] = (uint8_t)(p->order_cursor >> 8);
    out[4] = p->instr_cursor;
    out[5] = p->table;
    out[6] = p->table_row;
    out[7] = sv->voice;
    out[8] = sv->instr;
    out[9] = sv->l->theme;
    out[10] = sv->mute;
    out[11] = sv->solo;
    out[12] = sv->rec;
    out[13] = (uint8_t)song_view_playing(sv);
    out[14] = p->status;
    memcpy(out + 16u, &p->revision, sizeof(p->revision));
    memcpy(out + 20u, &sv->song_edits, sizeof(sv->song_edits));
    uintptr_t ptr = (uintptr_t)sv->song;
    memcpy(out + 24u, &ptr, sizeof(ptr));
    uint32_t bpm = song_view_shown_bpm(sv);
    out[32] = (uint8_t)bpm;
    out[33] = (uint8_t)(bpm >> 8);
    out[34] = p->arp;
    out[35] = p->arp_cur;
    out[36] = p->arp_draft.steps;
    out[37] = p->arp_draft.speed;
    out[38] = p->arp_draft.loop;
    out[39] = p->arp_start;
    memcpy(out + 40u, p->arp_draft.off, SONG_PAGES_ARP_STEPS);
}

static int model_loading(const song_view* sv) {
    return sv->io && (sv->io->state == SONG_IO_STOP || sv->io->state == SONG_IO_READ ||
                      sv->io->state == SONG_IO_VERIFY);
}

static int changed(screen* s, void* state) {
    (void)s;
    song_pages* p = state;
    if (model_loading(p->sv))
        return p->sv->which == p->page && p->sv->dirty != 0;
    uint8_t now[SONG_PAGES_LOOK];
    look(p, now);
    return !p->sv->built || p->sv->which != p->page || p->sv->dirty || p->unbuilt ||
           memcmp(now, p->drawn, sizeof(now)) != 0;
}

static void compose(screen* s, void* state, grid* g) {
    song_pages* p = state;
    song_view* sv = p->sv;
    if (model_loading(sv)) {
        if (sv->which == p->page)
            song_view_copy(sv, s, g);
        return;
    }
    (void)song_view_acquire_canvas(sv, p->page);
    if (p->page == SONG_VIEW_ORDER)
        sync_order(p);
    uint8_t now[SONG_PAGES_LOOK];
    look(p, now);
    if (!sv->built || sv->which != p->page || memcmp(now, p->drawn, sizeof(now)) != 0) {
        build_head(p);
        build_footer(p);
        p->unbuilt = (1u << SONG_PAGES_ROWS) - 1u;
        memcpy(p->drawn, now, sizeof(now));
    }
    if (p->unbuilt) {
        uint32_t first = 0u;
        while (!(p->unbuilt >> first & 1u))
            first++;
        uint32_t reach = screen_reach(s);
        uint32_t upto = screen_animating(s) ? (reach > SONG_PAGES_TOP ?
            (reach - SONG_PAGES_TOP + 1u) / 2u : 0u) : first + SONG_PAGES_BUILD_ROWS;
        if (upto > SONG_PAGES_ROWS)
            upto = SONG_PAGES_ROWS;
        for (uint32_t j = first; j < upto; j++) {
            build_row(p, j);
            p->unbuilt &= ~(1u << j);
            sv->dirty |= 3u << (SONG_PAGES_TOP + 2u * j);
        }
    }
    sv->built = 1u;
    song_view_copy(sv, s, g);
}

static void enter_page(song_pages* p, uint32_t page) {
    p->page = (uint8_t)page;
    p->unbuilt = 0u;
    if (p->sv->song && p->subtune >= p->sv->song->subtunes)
        p->subtune = 0u;
    p->sv->instr = (uint8_t)clamp(p->sv->instr, 1u, GT2_MAX_INSTR);
    if (page == SONG_VIEW_ORDER) {
        /* the cursor on the song views' entry, as TRACKER left it */
        if (!p->sv->song || p->sv->subtune < p->sv->song->subtunes)
            p->subtune = p->sv->subtune;
        p->order_cursor = (uint16_t)(p->sv->order_pos + 1u);
        sync_order(p);
    }
}

static void order_enter(screen* s, void* state) {
    (void)s;
    enter_page(state, SONG_VIEW_ORDER);
}

static void instr_enter(screen* s, void* state) {
    (void)s;
    enter_page(state, SONG_VIEW_INSTR);
}

static void table_enter(screen* s, void* state) {
    (void)s;
    enter_page(state, SONG_VIEW_TABLE);
}

static void leave(screen* s, void* state) {
    (void)s;
    song_pages* p = state;
    p->unbuilt = 0u;
    p->arp = 0u; /* the draft goes with the page */
}

/* A pop does not enter the revealed view, and MIDI learn calls the covered view directly.
 * Identity belongs to the callbacks, never the last page that used this shared state. */
#define PAGE_CALLBACKS(name, id) \
    static void name##_compose(screen* s, void* state, grid* g) { \
        ((song_pages*)state)->page = id; \
        compose(s, state, g); \
    } \
    static int name##_changed(screen* s, void* state) { \
        ((song_pages*)state)->page = id; \
        return changed(s, state); \
    }

PAGE_CALLBACKS(order, SONG_VIEW_ORDER)
PAGE_CALLBACKS(instr, SONG_VIEW_INSTR)
PAGE_CALLBACKS(table, SONG_VIEW_TABLE)

const view song_pages_order_view = {"ORDER", order_enter, leave, order_input, order_compose, order_changed};
const view song_pages_instr_view = {"INSTR", instr_enter, leave, instr_input, instr_compose, instr_changed};
const view song_pages_table_view = {"TABLE", table_enter, leave, table_input, table_compose, table_changed};
