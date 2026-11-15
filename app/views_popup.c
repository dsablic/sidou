// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "views_int.h"

static void ratio_text(char* buf, int32_t semis) {
    char num[3];
    views_number_text(num, (uint32_t)(semis < 0 ? -semis : semis));
    char sign[2] = {semis < 0 ? '-' : '+', 0};
    views_join(buf, SCREEN_POPUP_TEXT, sign, num);
}

static void pulse_text(char* buf, uint32_t pw) {
    char num[4];
    views_number_text(num, (pw * 99u + 2047u) / 4095u);
    views_join(buf, SCREEN_POPUP_TEXT, num, "%");
}

static void number_value(char* value, uint32_t n, uint32_t* fill, uint32_t* max, uint32_t top) {
    views_number_text(value, n);
    *fill = n;
    *max = top;
}

__attribute__((noinline, unused)) static int expert_value_text(uint32_t index, uint32_t raw,
                                                             const chip_setup* setup, char* name, char* value,
                                                             uint32_t* fill, uint32_t* max) {
    if (index >= SID_EXPERT_COUNT) {
        return 0;
    }
    views_join(name, SCREEN_POPUP_TEXT, chip_view_expert_name(index), "");
    const sid_chip_profile* p = sid_chip_profile_get(setup->profile);
    sid_model model = p ? (sid_model)p->model : SID_MODEL_6581;
    if (!sid_character_expert_active(model, index)) {
        views_join(value, SCREEN_POPUP_TEXT, "N/A", "");
        *fill = *max = 0;
        return 1;
    }
    chip_view_expert_setup_text(setup, index, value);
    *fill = raw;
    *max = SID_CHARACTER_MAX;
    return 1;
}

static int param_text(const views* v, uint32_t param, char* name, char* value, uint32_t* fill,
                      uint32_t* max) {
    const live* l = v->l;
    learn_view_param_name(param, name);
    if (param == LIVE_PARAM_CUTOFF) {
        live_view_cutoff_text(l, value);
        *fill = l->cutoff;
        *max = LIVE_CUTOFF_MAX;
    } else if (param == LIVE_PARAM_RESONANCE) {
        number_value(value, l->resonance, fill, max, LIVE_RESONANCE_MAX);
    } else if (param == LIVE_PARAM_PULSE_WIDTH) {
        pulse_text(value, l->pulse_width);
        *fill = l->pulse_width;
        *max = LIVE_PULSE_WIDTH_MAX;
    } else if (param == LIVE_PARAM_VOLUME) {
        views_join(name, SCREEN_POPUP_TEXT, "VOLUME", "");
        char pct[4];
        views_number_text(pct, l->volume);
        views_join(value, SCREEN_POPUP_TEXT, pct, "%");
        *fill = l->master;
        *max = LIVE_MASTER_MAX;
    } else if ((param >= LIVE_PARAM_AGE && param <= LIVE_PARAM_BLEED) || param == LIVE_PARAM_DECAY ||
               param == LIVE_PARAM_RELEASE) {
        number_value(value, live_param_get(l, param), fill, max, live_param_max(param));
    } else if (param >= LIVE_PARAM_SLOT && param < LIVE_PARAM_SLOT_END) {
        uint32_t i = param - LIVE_PARAM_SLOT;
        uint32_t slot = i / LIVE_SLOT_FIELDS;
        uint32_t field = i % LIVE_SLOT_FIELDS;
        uint32_t alg = l->slot[slot][LIVE_SLOT_ALGORITHM];
        char label[CHIP_VIEW_LABEL];
        char num[4];
        (void)chip_view_label(slot, alg, field, label);
        views_join(name, SCREEN_POPUP_TEXT, alg ? views_algorithm_name(slot, alg) : SLOT_NAMES[slot],
             alg ? "" : " OFF");
        number_value(num, l->slot[slot][field], fill, max, LIVE_SLOT_MAX);
        views_join(value, SCREEN_POPUP_TEXT, label, " ");
        views_append(value, SCREEN_POPUP_TEXT, num);
    } else if (param >= LIVE_PARAM_EXPERT && param < LIVE_PARAM_EXPERT_END) {
        chip_setup setup;
        chip_setup_of(l, &setup);
        setup.profile = l->profile_req;
        return expert_value_text(param - LIVE_PARAM_EXPERT, live_param_get(l, param),
                                 &setup, name, value, fill, max);
    } else {
        return 0;
    }
    return 1;
}

static int chip_choice_text(const views* v, char* name, char* value, int edit) {
    const chip_view* cv = &v->chip;
    const live* l = v->l;
    if (cv->tabs) {
        return 0;
    }
    if (!cv->chip && !cv->expert) {
        uint32_t k = l->fx_select % LIVE_SLOTS;
        views_join(name, SCREEN_POPUP_TEXT, SLOT_NAMES[k], "");
        views_join(value, SCREEN_POPUP_TEXT, views_algorithm_name(k, l->slot[k][LIVE_SLOT_ALGORITHM]),
             "");
        return 1;
    }
    char user[10] = "USER n";
    if (edit) {
        uint32_t t = CHIP_VIEW_TARGET(cv);
        user[5] = (char)('0' + t);
        if (t && !(l->user_valid >> (t - 1u) & 1u)) {
            views_append(user, sizeof user, " -");
        }
        views_join(name, SCREEN_POPUP_TEXT, "SAVE TO", "");
        views_join(value, SCREEN_POPUP_TEXT, t ? user : "OFF", "");
        return 1;
    }
    if (cv->expert) {
        char page[] = "1/4";
        page[0] = (char)('1' + cv->expert_page);
        page[2] = (char)('0' + CHIP_VIEW_EXPERT_PAGES);
        views_join(name, SCREEN_POPUP_TEXT, "EXPERT PAGE", "");
        views_join(value, SCREEN_POPUP_TEXT, page, "");
        return 1;
    }
    views_join(name, SCREEN_POPUP_TEXT, "PROFILE", "");
    uint32_t n = (uint32_t)chip_view_user(cv);
    user[5] = (char)('0' + n);
    if (n) {
        views_join(value, SCREEN_POPUP_TEXT, user, "");
    } else {
        views_profile_label(value, chip_view_song_tab(cv) ? cv->kr->song_want.profile : l->profile_req);
    }
    return 1;
}

static int menu_choice_text(const views* v, int set, char* name, char* value) {
    const menu_item* it = &(set ? &v->setup : &v->home)->items[v->menu_ask];
    if (!it->value) {
        return 0;
    }
    uint32_t i = v->menu_ask + (set ? VIEWS_MENU_ITEMS : 0u);
    views_join(name, SCREEN_POPUP_TEXT,
         i == VIEWS_MENU_PRESET ? "PRESET" : i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_ROWS ? "TRACKER ROWS" :
         i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_REC ? "REC QUANTISE" :
         i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_BLUETOOTH ? "BLUETOOTH" : it->label,
         "");
    char text[MENU_VALUE_CHARS];
    text[0] = 0;
    if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_THEME) {
        views_join(text, sizeof text, theme_get(v->l->theme)->name, "");
    } else if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_BLUETOOTH) {
        views_join(text, sizeof text, views_bluetooth_mode(v), "");
    } else if (set && v->format_arm) {
        views_join(name, SCREEN_POPUP_TEXT, v->format_arm == ARMED_RESET ? "RESET ALL?" : "WIPE STORE?", "");
        views_join(text, sizeof text, "SAVE AGAIN", "");
    } else if (i == VIEWS_MENU_ITEMS + VIEWS_SETTINGS_STORE && v->set && v->set->st && v->set->st->mount_newer &&
               store_state(v->set->st) == STORE_STATE_FOREIGN) {
        views_join(text, sizeof text, "NEWER SIDOU", "");
    } else {
        it->value((void*)v, text);
    }
    text[SCREEN_POPUP_CHARS] = 0;
    views_join(value, SCREEN_POPUP_TEXT, text, "");
    return 1;
}

int popup_text(void* ctx, uint32_t ask, char* name, char* value, uint32_t* fill,
               uint32_t* max, uint32_t* accent) {
    const views* v = ctx;
    const live* l = v->l;
    if (curtain(v) || learn_view_is_open(&v->learn)) {
        return 0;
    }
    if (ask & ASK_PARAM) {
        return param_text(v, ask & (ASK_PARAM - 1u), name, value, fill, max);
    }
    uint32_t enc = ask & 7u;
    if (enc == ASK_MASTER) {
        return param_text(v, LIVE_PARAM_VOLUME, name, value, fill, max);
    }
    if (views_shows(v) == &tracker_view_view &&
        (v->song.rec || v->song.gen.scl_down)) {
        *accent = theme_get(l->theme)->knob[enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4
                                                    ? enc - PANEL_ENC_K1 : 0u];
        return tracker_view_popup(&v->song, enc, (ask & ASK_EDIT) != 0u, name, value, fill, max);
    }
    if (views_shows(v) == &step_view_view &&
        step_view_popup(&v->step, enc, (ask & ASK_EDIT) != 0u, name, value, fill, max)) {
        *accent = theme_get(l->theme)->knob[enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4 ?
                                          enc - PANEL_ENC_K1 : 0u];
        return 1;
    }
    if (song_view_is(views_shows(v)) && views_shows(v) != &tracker_view_view &&
        views_shows(v) != &step_view_view) {
        if (song_pages_popup(&v->pages, views_shows(v), enc, (ask & ASK_EDIT) != 0u, name, value, fill, max)) {
            *accent = theme_get(l->theme)->knob[enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4 ?
                                              enc - PANEL_ENC_K1 : 0u];
            return 1;
        }
        if (enc == PANEL_ENC_K1 || enc == SONG_PAGES_ENC_K2) {
            return 0;
        }
    }
    if (views_shows(v) == &sid_view_playing && !(ask & ASK_EDIT) &&
        sid_view_popup(&v->sids, enc, name, value, fill, max)) {
        return 1;
    }
    if (enc >= PANEL_ENC_K1 && enc <= PANEL_ENC_K4) {
        *accent = theme_get(l->theme)->knob[enc - PANEL_ENC_K1];
        uint32_t held = ask & ASK_EDIT ? 1u << PANEL_BTN_EDIT : 0u;
        uint32_t param = panel_knob_param(v->p, l, held, enc);
        if (param != LIVE_PARAM_COUNT) {
            return param_text(v, param, name, value, fill, max);
        }
        if (v->p && v->p->page == PANEL_PAGE_NONE) {
            if (v->chip.expert) {
                uint32_t index = v->chip.expert_page * 4u + enc - PANEL_ENC_K1;
                return expert_value_text(index, chip_view_expert_value(&v->chip, index),
                                         &v->chip.kr->song_want,
                                         name, value, fill, max);
            }
            uint32_t k = enc - PANEL_ENC_K1;
            learn_view_param_name(LIVE_PARAM_AGE + k, name);
            number_value(value, v->chip.kr->song_want.controls[k], fill, max,
                         live_param_max(LIVE_PARAM_AGE + k));
            return 1;
        }
        if (v->p && v->p->page >= PANEL_PAGE_EXPERT_FIRST) {
            return 0;
        }
        views_join(name, SCREEN_POPUP_TEXT, "RATIO", "");
        ratio_text(value, l->sync_semis);
        *fill = (uint32_t)(l->sync_semis + LIVE_RATIO_SEMIS);
        *max = 2u * LIVE_RATIO_SEMIS;
        return 1;
    }
    if (enc == PANEL_ENC_PRESETS) {
        views_join(name, SCREEN_POPUP_TEXT, "PRESET", "");
        live_preset_name(l, value);
        return 1;
    }
    if (enc == PANEL_ENC_ALGORITHM) {
        const view* top = views_shows(v);
        if (top == &chip_view_view) {
            return chip_choice_text(v, name, value, (ask & ASK_EDIT) != 0u);
        }
        if (top == &views_menu || top == &views_settings_view) {
            return menu_choice_text(v, top == &views_settings_view, name, value);
        }
        if (top == &views_midi) {
            const menu_item* it = &v->midi.items[v->midi.cursor];
            views_join(name, SCREEN_POPUP_TEXT, it->label, "");
            char text[MENU_VALUE_CHARS] = {0};
            it->value((void*)v, text);
            text[SCREEN_POPUP_CHARS] = 0;
            views_join(value, SCREEN_POPUP_TEXT, text, "");
            return 1;
        }
    }
    return 0;
}
