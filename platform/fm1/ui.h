// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* The screen on the LCD: SIDOU's views (app/views.h) composed on the cell grid (app/grid.h) in
 * SIDOU's font and palette (docs/art/README.md), and the panel's buttons and encoders routed
 * through them. Main loop only; nothing here runs in an interrupt or from RAM. */
#ifndef SIDOU_FM1_UI_H
#define SIDOU_FM1_UI_H

#include <stdint.h>

#include "keys_route.h"
#include "live.h"
#include "player/song_ctl.h"
#include "ui_sound.h"
#include "panel.h"
#include "sid_transport.h"
#include "song_transport.h"
#include "song_io.h"

/* The screen is composed at most this often (25 fps); only cells that changed are sent. */
#define UI_PERIOD_MS 40u

/* After lcd_init: LIVE, the root view, of l and the audio's tap (audio_scope), with the UI
 * sounds us and the settings as the store kept them (app/settings.h, loaded here), under the
 * boot sequence (app/boot.h): the full one if home_held (HOME held at power-on) or once after
 * an install (no settings, or another version's), and its first frame composed at now_ms. It does not wait
 * on the panel: ui_poll sends that frame like any other (the whole screen, about 80 ms of
 * SPI) and turns the display on once it is out, so the main loop plays the keys and MIDI from
 * the start. now_ms is on ui_poll's clock: the main loop's uptime, which counts from TIMER4's
 * start, not from here. */
void ui_init(live* l, ui_sound* us, uint32_t now_ms, int home_held);
/* The two chips' routing, for the CHIP view's tabs (app/keys_route.h). */
void ui_set_keys(keys_route* kr);
/* The front panel, whose MASTER pot then asks for the value popup (views_set_panel). */
void ui_set_panel(panel* p);
/* The song's player for the song views (app/song_transport.h, main.c's binding over
 * song_ctl). */
void ui_set_song(const song_transport* t);
/* The .sid player (app/sid_transport.h, main.c's binding over the tune) for the SID views, GLO
 * and STATUS (views_set_tune). */
void ui_set_tune(const sid_transport* t);
/* A .sid tune takes the song's memory (on 1; M3, main.c): the song views lose their transport
 * and refuse to open (app/views.h views_tune). 0 while they cannot let it go now (an edit in
 * flight): ask again. Off: the transport ui_set_song gave back, the views open again. */
int ui_tune(int on);
/* A song read; back: song_io_returning's (the song views keep their places). */
void ui_song_loaded(int back);
void ui_set_song_io(song_io* io);
void ui_song_rows(uint8_t rows_per_beat);
settings* ui_settings(void);
/* The sound playing kept in USER sound slot (0..15) under name (app/user_sound.h's
 * user_sound_save): for the emulator tests' store hook (STORE_FM1_KEEP) until LAB's SAVE. */
int ui_keep_sound(uint32_t slot, const char* name);
/* Independent force ownership: a settings save cannot release a song save's holds. */
void ui_song_force(int on);
/* Main loop. Every UI_PERIOD_MS it runs a frame (views_frame: the view composes if what it
 * shows changed, app/screen.h), and half a frame later lets the view prepare the next one
 * (views_prepare), each in a call of its own. Changed cells go out as runs (one LCD window each), one per
 * call and only while the panel is free; the next run is rendered while the last one
 * transfers, so the main loop never waits on the panel. A call with nothing to send runs a
 * slice of the view's work between frames, if it has any (views_idle: RED MOON RITE's scan),
 * and returns; the first one after the first frame is out turns the display on. */
void ui_poll(uint32_t now_ms);
/* Main loop, after ui_poll: FM-1 Live Link's mirror of the grid (midi_link_run), and LINK in
 * the panel's bottom-left corner while a session runs (drawn on the panel only, over the
 * corner's two cells: the mirror shows the grid). */
void ui_link(uint32_t now_ms);
/* Main loop, once a pass: the song control and the events drained from its ring this pass,
 * for the views (views_song). */
void ui_song(const song_ctl* sc, const song_event* ev, uint32_t n);
/* Main loop, from input_poll: button press edges and the held buttons, and an encoder's
 * detents, through the views to the panel (views_buttons, views_encoder). */
void ui_buttons(panel* p, uint32_t pressed, uint32_t held);
void ui_encoder(panel* p, uint32_t enc, int32_t steps);
/* A toast over the view (app/screen.h screen_toast): text for ms from the next frame on. */
void ui_toast(const char* text, uint32_t ms);
/* Main loop, from input_poll after ui_buttons: the note keys through the views to the panel
 * (views_note_keys: the song views' jam, GLO's mutes and solos). */
void ui_note_keys(panel* p, uint32_t down, uint32_t pressed);
/* The update protocol's progress (views_update) in a frame over the view. Draws at once
 * (blocking): the main loop is inside the update session and does not call ui_poll. */
/* The LEDs' inputs from the views (views_leds). */
void ui_leds(uint32_t now_ms, panel_led_in* out);
void ui_update(uint32_t step, int32_t code);

#endif
