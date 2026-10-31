// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_VIEWS_H
#define SIDOU_VIEWS_H

#include <stdint.h>

#include "about.h"
#include "boot.h"
#include "chip_view.h"
#include "grid.h"
#include "help_view.h"
#include "lab_view.h"
#include "learn_view.h"
#include "live.h"
#include "live_view.h"
#include "midi_route.h"
#include "midi_sync.h"
#include "panel.h"
#include "save_key.h"
#include "scene_view.h"
#include "settings.h"
#include "sid_view.h"
#include "ui_sound.h"
#include "user_profile.h"
#include "user_sound.h"
#include "scope.h"
#include "screen.h"
#include "song_view.h"
#include "song_pages.h"
#include "step_view.h"
#include "theme.h"
#include "tracker_view.h"
#include "title_card.h"
#include "widget.h"

#ifdef __cplusplus
extern "C" {
#endif

#define VIEWS_TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
#define VIEWS_MENU_ITEMS 13u
#define VIEWS_MENU_CHIP 0u
#define VIEWS_MENU_TRACKER 1u
#define VIEWS_MENU_SCENE 2u
#define VIEWS_MENU_SONG 3u
#define VIEWS_MENU_LAB 4u
#define VIEWS_MENU_SIDS 5u
#define VIEWS_MENU_PRESET 6u
#define VIEWS_MENU_OCTAVE 7u
#define VIEWS_MENU_SHOWN 6u
#define VIEWS_MENU_SETTINGS 8u
#define VIEWS_MENU_STATUS 9u
#define VIEWS_MENU_MIDI 10u
#define VIEWS_MENU_CHARSET 11u
#define VIEWS_MENU_ABOUT 12u
#define VIEWS_SETTINGS_ITEMS 10u
#define VIEWS_SETTINGS_KEYS 0u
#define VIEWS_SETTINGS_SOUNDS 1u
#define VIEWS_SETTINGS_ROWS 2u
#define VIEWS_SETTINGS_CARDS 3u
#define VIEWS_SETTINGS_STORE 4u
#define VIEWS_SETTINGS_THEME 5u
#define VIEWS_SETTINGS_REC 6u
#define VIEWS_SETTINGS_SPREAD 7u
#define VIEWS_SETTINGS_SID_MODEL 8u
#define VIEWS_SETTINGS_RESET 9u
#define VIEWS_SETTINGS_LOOK 13u
#define VIEWS_MIDI_ITEMS 10u
#define VIEWS_MIDI_LOOK 14u
#define VIEWS_RASTER_STEPS SCREEN_RAMP
#define VIEWS_LOOK (19u + LIVE_VOICES + LIVE_SLOTS * LIVE_SLOT_FIELDS)

#define VIEWS_SONG_FILES 8u
typedef struct {
    const view* return_view;
    uint8_t page, cursor, action, confirm, direct, built;
    uint8_t scanning, skip, count, directory_page, io_state, drawn_theme;
    char after[17];
    char name[17];
    char files[VIEWS_SONG_FILES][17];
} views_song_menu;

typedef struct {
    screen s;
    save_key key;
    live* l;
    scope* sc;
    const char* version;
    menu home;
    uint8_t model;
    int8_t octave;
    uint8_t fx_select;
    uint8_t algorithm[LIVE_SLOTS];
    uint8_t drawn[VIEWS_LOOK];
    uint8_t format_arm;
    uint8_t resetting;
    panel* p;
    boot boot;
    chip_view chip;
    about about;
    learn_view learn;
    live_view live;
    scene_view scene;
    title_card card;
    song_view song;
    views_song_menu songs;
    uint8_t cards;
    ui_sound* snd;
    settings* set;
    uint8_t formatting;
    user_profiles users;
    uint16_t watched[4];
    screen_popup_state popup;
    midi_route* route;
    midi_sync* sync;
    menu midi;
    uint8_t midi_drawn[VIEWS_MIDI_LOOK];
    uint8_t menu_ask;
    song_editor editor;
    song_pages pages;
    uint32_t glo_tap_ms;
    uint32_t send_ask_at;
    uint8_t send_ask;
    step_view step;
    uint8_t cards_seen;
    uint8_t home_down;
    uint32_t home_at;
    help_view help;
    uint8_t tour;
    uint32_t tour_at;
    uint16_t tour_ref;
    uint16_t toast_held;
    const view* song_last;
    uint8_t tune;
    uint8_t status_page;
    menu setup;
    uint8_t setup_drawn[VIEWS_SETTINGS_LOOK];
    uint8_t led_head;
    uint32_t led_head_ms;
    sid_view sids;
    uint8_t tune_drawn[SID_VIEW_STATUS_LOOK];
    user_sounds sounds;
    lab_view lab;
} views;

extern const view views_status;
extern const view views_menu;
extern const view views_charset;
extern const view views_midi;
extern const view views_settings_view;
extern const view views_song_menu_view;

void views_init(views* v, grid* g, live* l, scope* sc, const char* version);
void views_song_loaded(views* v);
void views_song_back(views* v);
#define VIEWS_POPUP_MIDI 0
void views_popup_midi(views* v, int on);
void views_set_panel(views* v, panel* p);
void views_set_keys(views* v, keys_route* kr);
void views_set_sounds(views* v, ui_sound* us);
void views_route(views* v, midi_route* route);
void views_sync(views* v, midi_sync* sync);
void views_boot(views* v, int full, int home_held);
void views_learn(views* v, midi_learn* ml);
void views_set_song(views* v, const song_transport* t);
void views_tune(views* v, int on);
void views_set_tune(views* v, const sid_transport* t);
void views_song_io(views* v, song_io* io);
void views_note_keys(views* v, panel* p, uint32_t down, uint32_t pressed);
void views_settings(views* v, settings* set);
void views_sounds(views* v, store* st);
void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held);
void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps);
void views_prepare(views* v);
int views_idle(views* v);
void views_song(views* v, const song_ctl* sc, const song_event* ev, uint32_t n);
void views_leds(views* v, uint32_t now_ms, panel_led_in* out);
void views_flow(views* v, uint32_t now_ms);
void views_frame(views* v, uint32_t now_ms);
const char* views_slot_name(uint32_t slot);
const char* views_algorithm_name(uint32_t slot, uint32_t alg);
void views_update(views* v, uint32_t step, int32_t code);

#ifdef __cplusplus
}
#endif

#endif
