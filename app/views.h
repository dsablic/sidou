// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* SIDOU's views on the screen (app/screen.h) and the panel's routing through them
 * (docs/reference/controls.md).
 *
 * The views:
 * - LIVE, the root (app/live_view.h): the voices' oscilloscopes and envelopes, the filter's
 *   curve, the VU meters and the effect slots. What the screen shows from power-on; the tap
 *   on the audio runs only while it shows: on top, or under MIDI learn's frame.
 * - MENU (HOME), at 2 times the font, six of its items at a time: CHIP and TRACKER, the views
 *   to open, then PRE (PRESET), OCTAVE, CARDS, STORE (the store's free kilobytes, or FOREIGN:
 *   another firmware's data, which ALGORITHM turned clockwise twice formats), KEYS (AUTO or A:
 *   which chip the keys play during a song, app/keys_route.h), SOUNDS (the UI sounds on chip
 *   B's spare voice, OFF by default, app/ui_sound.h) and ROWS (TRACKER ROWS: BAR, the moving
 *   bar, or SCROLL: app/tracker_view.h), THEME, then STATUS, SCENE, CHARSET, SONG, ABOUT and MIDI. SELECT
 *   moves the selection (a line in reverse video; the list scrolls with it, a line or two a
 *   frame), ALGORITHM changes the value under it or opens the view. CARDS, KEYS, SOUNDS, ROWS and THEME
 *   persist (app/settings.h).
 * - CHIP (from the menu, behind THE CHIP's card): chip A's profile, character and effect slots
 *   (app/chip_view.h); SELECT moves the focus, ALGORITHM steps its choice, K1..K4 turn its
 *   values.
 * - STATUS (from the menu): the chip (with two chips both, KEYS marking the keys'), preset,
 *   octave, the sound parameters, the notes and the
 *   effect slots with every control, in numbers.
 * - SCENE (from the menu): generative pieces that react to the voices (app/scene_view.h);
 *   SELECT steps between them. The tap on the audio runs while it shows, as for LIVE.
 * - MIDI (from the menu): MIDI's mode and its options (app/midi_route.h), a menu like the
 *   main one: MODE, the base CHANNEL N, WHOLE-CHIP A's OPTION, LEGATO, the stored chord's two
 *   intervals, the ARP's rate in frames (0: CLOCK), the rows a beat for the clock, CLOCK
 *   (AUTO, FOLLOW, OFF) and CLOCK OUT (app/midi_sync.h). The page's setup stays in RAM;
 *   a song's MIDI chunk carries its own mode. Global MIDI defaults are not persisted.
 * - CHARSET: the font and the palette, the art reference.
 * - ABOUT: the credits, as a cracktro (app/about.h).
 * - SONG: LOAD and DELETE browse the store's songs; NEW confirms unsaved changes; SAVE AS
 *   edits a 16-character name (SELECT the cursor, ALGORITHM the character, SAVE writes it).
 *   Its large-text pages share the song views' canvas and their 200-cell frame budget; the
 *   directory keeps full 16-character filenames at the font's own size.
 * - HELP: the help card (app/help_view.h), over the view that shows, from HOME held (views_flow).
 * - BOOT: the boot sequence (app/boot.h), over LIVE at power-on (views_boot).
 * - LEARN: MIDI learn (app/learn_view.h), a frame over any view, from SAVE held while a knob
 *   turns (views_learn).
 * - The song views (app/song_view.h): STEP, TRACKER, ORDER, INSTR and TABLE, from SEQ in any
 *   view (the one that showed last, STEP the first time after power-on) or the menu's TRACKER;
 *   SEQ steps through them, HOME goes back to the view before.
 *   TRACKER shows the patterns (app/tracker_view.h); the other song views come later.
 * - TITLE: a title card (app/title_card.h) over a major view entered from the menu: THE CHIP
 *   over CHIP, THE SCENE over SCENE, THE OSCILLATOR over LIVE when HOME goes back to it from
 *   the menu (not at power-on, after the boot, nor when MIDI learn's frame closes). It ends by
 *   itself, on any control (which does nothing else) or on a note (which plays), and the view
 *   under it cuts in (SCREEN_BANDS). The menu's CARDS item: OFF, ONCE (the default: THE CHIP
 *   and THE SCENE the first time each since power-on, and never THE OSCILLATOR: the way back to
 *   LIVE is a wipe) or ALWAYS (all three, every time).
 *
 * Routing, for each button press and encoder turn:
 * 0. SAVE (app/save_key.h), before everything else: its press goes nowhere; its clean release
 *    saves: in MIDI learn's frame it forgets the picked parameter's CC (or confirms the clear),
 *    on the CHIP view's chip focus a USER slot, the save target or the applied one
 *    (chip_view_save; toast OVERWRITE USER 3? first when it holds a profile, then SAVED TO USER
 *    3 when the store commits), on a song view the song
 *    (song_io_save; an unnamed song opens SAVE AS),
 *    anywhere else the settings, now (settings_save_now; the toast SAVED when it commits).
 *    While SAVE is held, K1..K4 open LEARN on the knob's parameter, and HOME asks, in the frame,
 *    to clear every mapping (outside it, HOME does nothing). Over the boot or a title card SAVE
 *    is a control as any other: it ends them;
 * 1. the capture (screen_capture), if a view set one: it sees HOME too, so it can take HOME
 *    as "cancel";
 * 2. global (screen_global): HOME's press starts it (home_down) and its release, in the frame
 *    (views_flow), opens the menu on the root view and goes back one view anywhere else; held
 *    600 ms it opens the help card of the view that shows instead (app/help_view.h), and the
 *    release then does nothing; SEQ opens the song views or steps through them, PLAY plays or
 *    stops the song (EDIT held: goes on), REC turns record mode on and off (app/song_view.h);
 * 3. the view on top;
 * 4. the panel (app/panel.h): what nobody consumed plays the instrument as in M1a and M1c,
 *    in every view: OCT-/OCT+, FX, K1..K4, and the effect slots' EDIT shortcut (EDIT held +
 *    K1..K4, SCL, ENV), which stays global next to the CHIP view (docs/reference/controls.md
 *    says why). K1..K4 turn the panel's page (panel.page): the CHIP view's focused section
 *    while it shows (on top or under MIDI learn's frame), LIVE's knobs in every other view; the
 *    views set it before each event, so the panel and MIDI learn read the same one.
 * The note keys go through views_note_keys: the song views' jam and GLO's layer (the song's
 * mutes and solos) before the panel. MASTER does not pass through here.
 *
 * The views compose only when what they show changed (app/screen.h): STATUS, MENU and CHIP keep
 * a copy of the instrument's fields they show, MENU its cursor and blink as well, and CHARSET
 * never changes. An idle screen costs a comparison a frame. LIVE, SCENE and ABOUT move every
 * frame and compose every frame.
 *
 * Every frame the instrument's state is compared with the last frame's: a change of octave,
 * effect slot or algorithm shows a toast, whatever made it (panel, menu or MIDI), and a chip
 * model change also runs the raster over the border; not while the boot shows, which nothing
 * draws over (its changes are taken as seen). A control's own popup (below) says what it
 * changed: no toast then.
 *
 * The value popup (app/screen.h): a K knob, ALGORITHM or the MASTER pot changes a parameter, and
 * the screen shows its name and value in the largest text over the view (popup_text, the one
 * function that names and formats a control: the views' own text, from live's state). The
 * input paths only store which control changed (views_encoder, master_moved); the frame asks.
 * SELECT never does, and neither does MIDI (VIEWS_POPUP_MIDI, views_popup_midi).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_VIEWS_H
#define SIDOU_VIEWS_H

#include <stdint.h>

#include "about.h"
#include "boot.h"
#include "chip_view.h"
#include "grid.h"
#include "help_view.h"
#include "learn_view.h"
#include "live.h"
#include "live_view.h"
#include "midi_route.h"
#include "midi_sync.h"
#include "panel.h"
#include "save_key.h"
#include "scene_view.h"
#include "settings.h"
#include "ui_sound.h"
#include "user_profile.h"
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

/* The status screen's colours: light blue on blue. */
#define VIEWS_TEXT GRID_ATTR(GRID_LIGHT_BLUE, GRID_BLUE)
/* The menu's items, the title cards' option, and the items that open a view. */
#define VIEWS_MENU_ITEMS 17u
#define VIEWS_MENU_CHIP 0u
#define VIEWS_MENU_TRACKER 1u
#define VIEWS_MENU_CARDS 9u
#define VIEWS_MENU_STORE 10u
#define VIEWS_MENU_KEYS 6u
#define VIEWS_MENU_SOUNDS 7u
#define VIEWS_MENU_ROWS 8u
#define VIEWS_MENU_THEME 11u
/* PRESET (the item that shows PRE), and the items the menu shows at once. */
#define VIEWS_MENU_PRESET 4u
#define VIEWS_MENU_OCTAVE 5u
#define VIEWS_MENU_SHOWN 6u
#define VIEWS_MENU_STATUS 12u
#define VIEWS_MENU_SCENE 2u
#define VIEWS_MENU_CHARSET 14u
#define VIEWS_MENU_SONG 3u
#define VIEWS_MENU_REC 15u
#define VIEWS_MENU_ABOUT 16u
#define VIEWS_MENU_MIDI 13u
/* The MIDI page's items (app/midi_route.h): MODE, CHANNEL, OPTION, LEGATO, CHORD 2, CHORD 3,
 * ARP RATE, ROWS/BEAT, and the clock's (app/midi_sync.h): CLOCK, CLOCK OUT. */
#define VIEWS_MIDI_ITEMS 10u
/* What it shows: the setup's 8 bytes, the cursor, the first row, the blink, whether it has
 * a router, CLOCK and CLOCK OUT. */
#define VIEWS_MIDI_LOOK 14u
/* Raster steps for a chip model change: the ramp once round. */
#define VIEWS_RASTER_STEPS SCREEN_RAMP
/* What the views show, as bytes (app/views.c look): the instrument's model (as asked for),
 * which chip the keys play (with two chips and the KEYS setting), the keys' profile and the
 * other chip's, the preset, octave, cutoff (2), resonance, pulse width (2), volume and selected slot, each
 * voice's sounding note, every slot field, the title cards' option, the menu's cursor,
 * first row and blink, then the store's state and free kilobytes (only MENU compares these two). */
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
    scope* sc; /* the audio's tap, NULL: none; on while LIVE or SCENE shows */
    const char* version; /* "V" and the version, at most 14 characters */
    menu home;
    /* The instrument as the last frame saw it, for the toasts. */
    uint8_t model;
    int8_t octave;
    uint8_t fx_select;
    uint8_t algorithm[LIVE_SLOTS];
    /* What the view on top showed at its last compose: it composes again when this changes
     * (the views' changed functions). */
    uint8_t drawn[VIEWS_LOOK];
    uint8_t format_arm; /* the menu's STORE on a foreign store: turned clockwise once */
    panel* p; /* the panel of the last event (views_buttons, views_encoder); NULL before */
    boot boot;
    chip_view chip;
    about about;
    learn_view learn;
    live_view live;
    scene_view scene;
    title_card card;
    song_view song;
    views_song_menu songs;
    uint8_t cards; /* the menu's CARDS: 0 OFF, 1 ONCE, 2 ALWAYS */
    ui_sound* snd; /* the UI sounds (the menu's SOUNDS, app/ui_sound.h); NULL: none */
    settings* set; /* what SAVE saves and the menu's STORE shows (views_settings); NULL: none */
    uint8_t formatting; /* a format's erases run (forced: the user asked) */
    user_profiles users; /* the CHIP view's USER slots on the store (app/user_profile.h) */
    uint16_t watched[4]; /* the parameters views_popup_midi watches, as the last frame saw them */
    /* The popup's state (app/screen.h), here at the end so that the screen's own fields, and the
     * views' before this, keep their small offsets. */
    screen_popup_state popup;
    midi_route* route; /* MIDI's mode, for the MIDI page (views_route); NULL: none */
    midi_sync* sync;   /* MIDI's clock, for the MIDI page's CLOCK items; NULL: none */
    menu midi;
    uint8_t midi_drawn[VIEWS_MIDI_LOOK];
    uint8_t menu_ask; /* the item ALGORITHM changed, retained while its popup waits */
    song_editor editor;
    song_pages pages;
    /* When GLO was last pressed, plus 1 (0: never), for the double tap that latches its layer. */
    uint32_t glo_tap_ms;
    step_view step;
    uint8_t cards_seen; /* ONCE: bit id, the card id has played since power-on */
    /* HOME acts on its release (views_flow): 0 up, 1 pressed (it goes home when let go, or opens
     * the help card when held long enough), 2 held and a help card opened (the release only
     * clears it). */
    uint8_t home_down;
    uint32_t home_at; /* the frame time of its press, ms */
    help_view help;
    /* The first-run tour (app/views.c tour_frame): 0 off, else its step; when that step began
     * (ms) and what it watches for a change. Not saved: the version is, after the boot. */
    uint8_t tour;
    uint32_t tour_at;
    uint16_t tour_ref;
    /* A toast raised while a help card shows waits under it: the ms it has left (0 none), its
     * text in the screen's toast (views_frame). */
    uint16_t toast_held;
    /* The song view that showed last (views_frame), which SEQ reopens from outside them; NULL
     * until one has shown since power-on (SEQ then opens STEP). Not saved. */
    const view* song_last;
} views;

extern const view views_status;
extern const view views_menu;
extern const view views_charset;
extern const view views_midi;
extern const view views_song_menu_view;

/* LIVE, the root view, on g, showing l and the audio's tap sc (NULL: none). */
void views_init(views* v, grid* g, live* l, scope* sc, const char* version);
void views_song_loaded(views* v);
/* As views_song_loaded, for a read song_io_returning calls returning (About's tune and the song
 * it stood in for): the song views keep their cursors, the instrument, the subtune, STEP's lanes
 * and page and the pages' cursors; what read the song in between starts again. */
void views_song_back(views* v);
/* Whether MIDI's changes of the knobs' parameters (cutoff, resonance, pulse width, volume) open
 * the value popup: 0, they do not (a controller sweep or a song would flood the screen with it).
 * The one switch: 1 here, or views_popup_midi at run time, turns the watcher on; nothing else
 * decides it. The panel's controls always do. */
#define VIEWS_POPUP_MIDI 0
/* 1: a change of the cutoff, resonance, pulse width or volume that no panel control made (MIDI,
 * a song) opens the popup too, from the next frame; 0: it does not (views_init's default,
 * VIEWS_POPUP_MIDI). */
void views_popup_midi(views* v, int on);
/* The panel whose MASTER pot asks for the popup (panel.master_hook); until set, it does not. */
void views_set_panel(views* v, panel* p);
/* The two chips' routing (app/keys_route.h), for the CHIP view's tabs; NULL until set. */
void views_set_keys(views* v, keys_route* kr);
/* The UI sounds (app/ui_sound.h): the menu's SOUNDS turns them on and off, and the menu and
 * the toasts play them; NULL until set (and the SOUNDS item then reads OFF and does nothing). */
void views_set_sounds(views* v, ui_sound* us);
/* MIDI's mode (app/midi_route.h), which the MIDI page edits; NULL until set (the page then
 * shows "-" and changes nothing). Binding also sets the song header's rows a beat. */
void views_route(views* v, midi_route* route);
/* MIDI's clock (app/midi_sync.h): the MIDI page's CLOCK and CLOCK OUT; NULL until set (they
 * show "-"). */
void views_sync(views* v, midi_sync* sync);
/* The boot sequence over it from the next frame on: the full one if full (the first boot
 * after an install) or if HOME was held at power-on (home_held), whose first HOME press edge is
 * then part of the hold (app/boot.h). The full boot of an install, with HOME not held, is
 * followed by the first-run tour: three toasts, TURN K1, PRESS ENV, HOLD HOME FOR HELP
 * (views_flow). */
void views_boot(views* v, int full, int home_held);
/* SAVE held + a knob opens MIDI learn over ml's bindings from now on (app/learn_view.h); NULL:
 * it does nothing (views_init's default). */
void views_learn(views* v, midi_learn* ml);
/* The song's player (app/song_transport.h) for the song views; NULL: none (the host tests'
 * default; the device binds song_ctl, platform/fm1/main.c). */
void views_set_song(views* v, const song_transport* t);
/* The song files' controller; NULL until the platform binds it. */
void views_song_io(views* v, song_io* io);
/* The note keys, as panel_note_keys takes them, after the frame's buttons (views_buttons):
 * in a song view with a player, the jam takes them (song_view_keys); GLO's layer takes its
 * keys to the song's mutes and solos (song_view_glo); the rest play the instrument. */
void views_note_keys(views* v, panel* p, uint32_t down, uint32_t pressed);
/* The settings (app/settings.h, made with this views' cards): SAVE saves them, views_frame
 * ticks them, the menu's STORE shows their store. NULL (views_init's default): SAVE saves
 * nothing and STORE reads NONE. */
void views_settings(views* v, settings* set);
/* Button press edges and the held buttons, as panel_buttons takes them: routed as above, and
 * what nobody consumed goes on to panel_buttons. */
void views_buttons(views* v, panel* p, uint32_t pressed, uint32_t held);
/* Detent steps of encoder enc, as panel_encoder takes them. */
void views_encoder(views* v, panel* p, uint32_t enc, int32_t steps);
/* Half way between two frames: LIVE or SCENE, showing (on top or under MIDI learn's frame) and
 * still, prepares its next frame (live_view_prepare, scene_view_prepare). */
void views_prepare(views* v);
/* A slice of a view's work between frames, in a ui_poll that has nothing to send and no frame
 * or preparation due (RED MOON RITE's scan, scene_view_idle): 1 while work is left. */
int views_idle(views* v);
/* Once a main-loop pass: the song control (NULL: none) and the events its ring gave this
 * pass (platform/fm1/main.c drains it, the ring's one reader, and hands each reader them). */
void views_song(views* v, const song_ctl* sc, const song_event* ev, uint32_t n);
/* The flow step of a frame (views_frame runs it when HOME is down or a help card is up; the host
 * tests run it alone, after a tap of HOME, as the next frame would): HOME let go goes home (the
 * menu from LIVE, back one view elsewhere), HOME held 600 ms opens the help card of the view
 * that shows, a help card that was closed is popped. Nothing before the first event (no panel
 * yet). */
void views_flow(views* v, uint32_t now_ms);
/* One frame: the boot's progress, a title card's end, the toasts and raster of what changed,
 * MIDI learn's end (learn_view_frame), then screen_frame. */
void views_frame(views* v, uint32_t now_ms);
/* The effect slots' names as the screens show them (at most 9 characters); "?" for an unknown
 * slot or algorithm. */
const char* views_slot_name(uint32_t slot);
const char* views_algorithm_name(uint32_t slot, uint32_t alg);
/* The update protocol's progress (Felucca's ota_show steps: 1 package, 2 head, 3 loader,
 * 4 confirm, 5 restart; 9 the result, code 1 a dry run, < 0 a failure) in a frame over the
 * view, straight onto the grid: the main loop is inside the update session and runs no
 * frames. The next frame composes the view again. */
void views_update(views* v, uint32_t step, int32_t code);

#ifdef __cplusplus
}
#endif

#endif
