// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* RED MOON RITE (M2f Task 4): two barbarians duel under a blood moon, and the music plays the
 * fight (docs/art/README.md "RED MOON RITE"). The prototype's logic in C: red-moon-rite.js's
 * step (a step a row of the music) and its layers' and stages' state (a frame), with the
 * disarm in place of the prototype's helmet knock; then the frame as layers on the tile
 * compositor (app/scene_px.h).
 *
 * What the music does (scene_rmr_beat, from app/scene_song.h):
 * - a kick: the leader raises his sword and chops overhead; a snare: the other parries, a
 *   spark where the blades meet, and the leader loses a stamina notch; a hat: both shift
 *   between their two guards; the bass: high gives the right barbarian (1) the lead, low the
 *   left (0), and the pair drifts toward the one giving ground; an arp note: the torches
 *   flare;
 * - the fill (the last beat of every fourth bar): the leader's spinning attack, four rows,
 *   lands and disarms the other: his sword flies up spinning (the eight scene_rmr_sword
 *   frames, scene_rmr_flights' path) and sticks point-down in the flagstones with a small
 *   shake; he stands unarmed, and on the first beat after it has stuck he bends (reach) and
 *   pulls it free. While one is unarmed nobody attacks or parries;
 * - each bar's first row refills an empty stamina meter;
 * - the cutoff (0..2047): the sky's palette, from the darkest (0, below SCENE_RMR_SKY_DIM) to
 *   the brightest (2), wiped down a cell row a frame.
 * Everything is integer and seeded by nothing: the same music gives the same frames.
 *
 * Silence calms it: SCENE_RMR_CALM_FRAMES frames after the last gate went off, the torches and
 * the spin's arc hold still, and once the timers have run out nothing changes, so nothing is
 * sent. The state machine itself (scene_rmr_step, scene_rmr_frame) is the script's, which the
 * host tests check frame by frame against the script (tests/golden/scene/rmr-trace.tsv).
 *
 * Portable C with no hardware access and no allocation. */
#ifndef SIDOU_SCENE_RMR_H
#define SIDOU_SCENE_RMR_H

#include <stdint.h>

#include "scene_art.h"
#include "scene_px.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The poses: scene_art.h's armed ones, then reach (unarmed only): SCENE_RMR_POSES. */
#define SCENE_RMR_REACH 8u

/* A beat's hits. */
#define SCENE_RMR_KICK 1u
#define SCENE_RMR_SNARE 2u
#define SCENE_RMR_HAT 4u
#define SCENE_RMR_ARP 8u
enum { SCENE_RMR_BASS_NONE, SCENE_RMR_BASS_LOW, SCENE_RMR_BASS_HIGH };

/* The pair's middle at the start, and how far it drifts each way. */
#define SCENE_RMR_MID 120
#define SCENE_RMR_DRIFT 8
/* Each fighter's place, either side of the middle. */
#define SCENE_RMR_APART 30
#define SCENE_RMR_HP 8u
/* The sky's cell rows that wipe: 2..16 (row 16's palette also shows on row 17). */
#define SCENE_RMR_SKY_ROW0 2u
#define SCENE_RMR_SKY_ROWS 15u
/* The cutoff from which the sky takes palettes 1 and 2: the prototype's 0.36 and 0.68 of it. */
#define SCENE_RMR_SKY_DIM 737u
#define SCENE_RMR_SKY_MID 1392u
/* Frames with no gate after which the torches and the spin's arc hold still (a second). */
#define SCENE_RMR_CALM_FRAMES 25u
/* The spin's arc: its pixels. */
#define SCENE_RMR_ARC_MARKS 14u
/* Where the blades meet, above the feet, and the arc's height. */
#define SCENE_RMR_SPARK_UP 92
#define SCENE_RMR_ARC_UP 60

/* The layer slots (app/scene_px.h), bottom first; SCENE_RMR_SLOT_COVER is the view's (what
 * the screen draws over the scene: a toast, MIDI learn's frame, the raster). */
enum {
    SCENE_RMR_SLOT_HUD,
    SCENE_RMR_SLOT_EMPTY0,
    SCENE_RMR_SLOT_FULL0,
    SCENE_RMR_SLOT_TIP0,
    SCENE_RMR_SLOT_EMPTY1,
    SCENE_RMR_SLOT_FULL1,
    SCENE_RMR_SLOT_TIP1,
    SCENE_RMR_SLOT_SKY,
    SCENE_RMR_SLOT_ARENA,
    SCENE_RMR_SLOT_FLAME0,
    SCENE_RMR_SLOT_FLAME1,
    SCENE_RMR_SLOT_STUCK_BEHIND,
    SCENE_RMR_SLOT_SHADOW0,
    SCENE_RMR_SLOT_FIGHTER0,
    SCENE_RMR_SLOT_SHADOW1,
    SCENE_RMR_SLOT_FIGHTER1,
    SCENE_RMR_SLOT_SPARK,
    SCENE_RMR_SLOT_ARC,
    SCENE_RMR_SLOT_STUCK_OVER,
    SCENE_RMR_SLOT_SWORD,
    SCENE_RMR_SLOT_COVER,
    SCENE_RMR_SLOTS
};

/* One step of the music: a row (row 1: s and bar are its place), or hits between rows (row
 * 0: the instrument's notes with no song, app/scene_song.h). */
typedef struct {
    uint8_t row;
    uint8_t s;    /* the row in its bar, 0 .. 4 * rpb - 1 */
    uint8_t bar;  /* 0..3: the fill is bar 3's last beat */
    uint8_t rpb;  /* rows a beat, 1.. */
    uint8_t hits; /* SCENE_RMR_KICK, SNARE, HAT, ARP */
    uint8_t bass; /* SCENE_RMR_BASS_* */
} scene_rmr_beat;

typedef struct {
    int16_t x;        /* the pair's middle */
    uint8_t lead;     /* the fighter who attacks */
    uint8_t pose[2];
    uint8_t t[2];     /* frames the pose holds; 0: back to a guard */
    uint8_t next;     /* the pose a raise turns into, + 1; 0 none */
    uint8_t stance;   /* guard (0) or guard2 (1) */
    uint8_t hp[2];
    int8_t unarmed;   /* the fighter without his sword, -1 none */
    uint8_t sword;    /* his sword is out of his hand */
    uint8_t sword_n;  /* frames since it flew (scene_rmr_flights' frames: stuck) */
    int16_t sword_x;  /* his place when it flew */
    uint8_t spark;    /* frames of the parry's spark left */
    uint8_t spin;     /* rows of the spinning attack left */
    uint8_t spin_who;
    uint8_t shake;
    uint8_t flare;
    uint8_t sky[SCENE_RMR_SKY_ROWS]; /* each wiping row's palette */
    uint32_t frame;   /* frames run */
    /* What the last frame shows (scene_rmr_frame), for the layers. */
    int8_t shake_dx;  /* the fighters' shake */
    uint8_t spark_shown; /* the spark's stage, 0 none */
    uint8_t flame_big;
    uint8_t arc;      /* the spin's arc shows */
    uint8_t sky_row;  /* the cell row the frame's wipe changed, 0xFF none */
    uint8_t quiet;    /* frames with no gate, up to SCENE_RMR_CALM_FRAMES */
    uint32_t anim;    /* frames the torches and the arc have moved */
    scene_px_mark arc_marks[SCENE_RMR_ARC_MARKS];
} scene_rmr;

/* The start: the pair in guard in the middle, full meters, the sky in palette 1. */
void scene_rmr_init(scene_rmr* r);
/* A step of the music (the script's step). */
void scene_rmr_step(scene_rmr* r, const scene_rmr_beat* e);
/* A frame (the script's layers and stages): the sky's wipe toward the cutoff's palette, the
 * torches, the poses' timers, the drift, the shake, the sword and the spark. sounding: a
 * voice's gate is on (silence calms it). */
void scene_rmr_frame(scene_rmr* r, uint32_t cutoff, int sounding);
/* The sky palette (0..2) the cutoff (0..2047) asks for. */
uint32_t scene_rmr_sky_of(uint32_t cutoff);
/* The frame onto sp's slots 0 .. SCENE_RMR_SLOT_COVER - 1: a slot whose layer is the same as
 * the last frame's makes no candidates; the sky's wiped row and the torches' cycling touch
 * their tiles. */
void scene_rmr_draw(scene_rmr* r, scene_px* sp);
/* The sword's grip and rotation while it flies (k 0..7), or where it sticks (k 8); 0 when
 * the sword is in a hand. */
int scene_rmr_sword_at(const scene_rmr* r, int32_t* gx, int32_t* gy, uint32_t* k);

#ifdef __cplusplus
}
#endif

#endif
