// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
/* RED MOON RITE's music hooks (app/scene_song.h). */
#include "scene_song.h"

#include <stddef.h>

#include "live_view.h"
#include "song/song_ext.h"

#define NONE SCENE_SONG_NO_NOTE
/* A GT2 note (0 is C-0) as a MIDI note. */
#define MIDI_OF(note) ((uint32_t)(note) + 12u)

void scene_song_init(scene_song* ss) {
    static const scene_song empty;
    *ss = empty;
    ss->rpb = SCENE_SONG_RPB;
    ss->bar = 3; /* the first bar's first row makes it bar 0 */
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        ss->note[v] = NONE;
        ss->low[0][v] = ss->low[1][v] = NONE;
    }
    ss->blo[0] = ss->blo[1] = NONE;
}

uint32_t scene_song_drum_of_pitch(uint32_t pitch) {
    return pitch < SCENE_SONG_KICK_BELOW ? SONG_DRUM_KICK
           : pitch >= SCENE_SONG_HAT_FROM ? SONG_DRUM_HAT
                                          : SONG_DRUM_SNARE;
}

/* A play started: the song's chunks, and the place and history from nothing. */
static void setup(scene_song* ss, const gt2_song* s) {
    const song_ctl* sc = ss->sc;
    scene_song_init(ss);
    ss->sc = sc;
    ss->playing = 1;
    song_scene scne;
    if (s && song_ext_get_scene(s, &scne)) {
        for (uint32_t v = 0; v < SCENE_SONG_VOICES && v < scne.count; v++) {
            ss->role[v] = scne.role[v];
        }
        ss->drums = scne.drums;
        for (uint32_t i = 0; i < scne.drums; i++) {
            ss->drum[i][0] = scne.drum[i][0];
            ss->drum[i][1] = scne.drum[i][1];
        }
    }
    song_midi midi;
    if (s && song_ext_get_midi(s, &midi) && midi.rows_per_beat >= 1u && midi.rows_per_beat <= 16u) {
        ss->rpb = midi.rows_per_beat;
    }
}

/* Instrument i's first wavetable step: its left (waveform) and right (note) bytes. */
static int first_wave(const gt2_song* s, uint32_t i, uint8_t* left, uint8_t* right) {
    if (!s || i < 1u || i > GT2_MAX_INSTR) {
        return 0;
    }
    uint32_t w = s->instr[i - 1u][GT2_INS_WAVE];
    if (w < 1u || w > GT2_TABLE_ROWS) {
        return 0;
    }
    *left = s->ltable[GT2_WTBL][w - 1u];
    *right = s->rtable[GT2_WTBL][w - 1u];
    return 1;
}

/* The instrument's wavetable starts with noise (a waveform byte, $10-$DF, with bit 7). */
static int starts_with_noise(const gt2_song* s, uint32_t i) {
    uint8_t l, r;
    return first_wave(s, i, &l, &r) && l >= 0x80u && l <= 0xdfu;
}

/* A drum's pitch (MIDI): the first step's note, absolute or relative to the row's. */
static uint32_t drum_pitch(const gt2_song* s, uint32_t i, uint32_t note) {
    uint8_t l, r;
    if (!first_wave(s, i, &l, &r) || r == 0x80u) {
        return MIDI_OF(note);
    }
    return MIDI_OF(r > 0x80u ? r - 0x80u : (r + note) & 0x7fu);
}

static uint32_t drum_of(const scene_song* ss, const gt2_song* s, uint32_t instr, uint32_t note) {
    for (uint32_t i = 0; i < ss->drums; i++) {
        if (ss->drum[i][0] == instr && ss->drum[i][1] != SONG_DRUM_NONE) {
            return ss->drum[i][1];
        }
    }
    return scene_song_drum_of_pitch(drum_pitch(s, instr, note));
}

static uint32_t hit_of(uint32_t drum) {
    return drum == SONG_DRUM_KICK ? SCENE_RMR_KICK
           : drum == SONG_DRUM_SNARE ? SCENE_RMR_SNARE
                                     : SCENE_RMR_HAT;
}

static void push(scene_song* ss, const scene_rmr_beat* b) {
    if (ss->n >= SCENE_SONG_BEATS) {
        for (uint32_t i = 1; i < SCENE_SONG_BEATS; i++) {
            ss->beat[i - 1u] = ss->beat[i];
        }
        ss->n--;
        ss->lost++;
    }
    ss->beat[ss->n++] = *b;
}

/* A new bar: this bar's history becomes the last's. */
static void new_bar(scene_song* ss) {
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        ss->noise[1][v] = ss->noise[0][v];
        ss->noise[0][v] = 0;
        ss->low[1][v] = ss->low[0][v];
        ss->low[0][v] = NONE;
    }
    ss->blo[1] = ss->blo[0];
    ss->bhi[1] = ss->bhi[0];
    ss->blo[0] = NONE;
    ss->bhi[0] = 0;
}

/* The bass's note against its range over this bar and the last. */
static uint8_t bass_of(scene_song* ss, uint32_t pitch) {
    uint8_t p = (uint8_t)(pitch < 0xffu ? pitch : 0xfeu);
    if (p < ss->blo[0]) {
        ss->blo[0] = p;
    }
    if (p > ss->bhi[0]) {
        ss->bhi[0] = p;
    }
    uint32_t lo = ss->blo[0] < ss->blo[1] ? ss->blo[0] : ss->blo[1];
    uint32_t hi = ss->bhi[0] > ss->bhi[1] ? ss->bhi[0] : ss->bhi[1];
    int high = lo < hi ? 2u * pitch > lo + hi : pitch >= SCENE_SONG_BASS_SPLIT;
    return high ? SCENE_RMR_BASS_HIGH : SCENE_RMR_BASS_LOW;
}

/* The voices' roles now: the chunk's, else guessed (noise DRUMS, the lowest BASS). */
static void roles(const scene_song* ss, uint8_t out[SCENE_SONG_VOICES]) {
    uint32_t bass = SCENE_SONG_VOICES;
    uint32_t lowest = NONE;
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        out[v] = ss->role[v];
        if (out[v] != SONG_ROLE_AUTO) {
            continue;
        }
        if (ss->noise[0][v] | ss->noise[1][v]) {
            out[v] = SONG_ROLE_DRUMS;
            continue;
        }
        uint32_t lo = ss->low[0][v] < ss->low[1][v] ? ss->low[0][v] : ss->low[1][v];
        if (lo < lowest) {
            lowest = lo;
            bass = v;
        }
    }
    int chunk_bass = 0;
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        chunk_bass |= ss->role[v] == SONG_ROLE_BASS;
    }
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        if (out[v] == SONG_ROLE_AUTO) {
            out[v] = v == bass && !chunk_bass ? SONG_ROLE_BASS : SONG_ROLE_ARP;
        }
    }
}

/* The call gathered so far, as a step. */
static void flush(scene_song* ss) {
    if (!ss->open) {
        return;
    }
    ss->open = 0;
    const gt2_song* s = ss->sc ? ss->sc->song : NULL;
    scene_rmr_beat b = {0, 0, 0, ss->rpb, 0, SCENE_RMR_BASS_NONE};
    if (ss->row_ev) {
        uint32_t s4 = ss->row % (4u * ss->rpb);
        if (!ss->rows_seen || s4 <= ss->s) {
            ss->bar = (uint8_t)((ss->bar + 1u) & 3u);
            new_bar(ss);
        }
        ss->rows_seen = 1;
        ss->s = (uint8_t)s4;
        b.row = 1;
        b.s = ss->s;
        b.bar = ss->bar;
    }
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        if (ss->note[v] == NONE) {
            continue;
        }
        if (starts_with_noise(s, ss->instr[v])) {
            ss->noise[0][v] = 1;
        }
        uint8_t p = (uint8_t)MIDI_OF(ss->note[v]);
        if (p < ss->low[0][v]) {
            ss->low[0][v] = p;
        }
    }
    uint8_t role[SCENE_SONG_VOICES];
    roles(ss, role);
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        if (ss->note[v] == NONE) {
            continue;
        }
        switch (role[v]) {
        case SONG_ROLE_DRUMS:
            b.hits |= (uint8_t)hit_of(drum_of(ss, s, ss->instr[v], ss->note[v]));
            break;
        case SONG_ROLE_BASS: b.bass = bass_of(ss, MIDI_OF(ss->note[v])); break;
        case SONG_ROLE_ARP:
        case SONG_ROLE_LEAD: b.hits |= SCENE_RMR_ARP; break;
        default: break;
        }
        ss->note[v] = NONE;
    }
    if (b.row || b.hits || b.bass != SCENE_RMR_BASS_NONE) {
        push(ss, &b);
    }
}

void scene_song_events(scene_song* ss, const song_ctl* sc, const song_event* ev, uint32_t n) {
    ss->sc = sc;
    int playing = sc && song_ctl_state(sc) == SONG_PLAYING;
    if (!playing) {
        ss->playing = 0;
        return;
    }
    if (!ss->playing) {
        setup(ss, sc->song);
    }
    uint32_t quiet = (uint32_t)sc->mute | sc->taken;
    for (uint32_t i = 0; i < n; i++) {
        const song_event* e = &ev[i];
        uint32_t ch = SONG_EV_CHANNEL(e), f = SONG_EV_FLAGS(e);
        if (ch >= SCENE_SONG_VOICES) {
            continue;
        }
        if (ss->open && e->frame != ss->call) {
            flush(ss);
        }
        if (!ss->open) {
            ss->open = 1;
            ss->call = e->frame;
            ss->row_ev = 0;
        }
        if ((f & GT2_EV_ROW) && ch == SCENE_SONG_CLOCK) {
            ss->row_ev = 1;
            ss->row = e->row;
        }
        if (quiet >> ch & 1u) {
            ss->gate[ch] = 0;
            continue;
        }
        if (f & GT2_EV_NOTE) {
            ss->note[ch] = e->note;
            ss->instr[ch] = e->instr;
        }
        if (f & GT2_EV_GATE_ON) {
            ss->gate[ch] = 1;
        }
        if (f & GT2_EV_GATE_OFF) {
            ss->gate[ch] = 0;
        }
    }
}

/* With no song: the instrument's note-ons since the last frame, and the free clock. */
static void from_live(scene_song* ss, const live* l, scene_song_frame_out* out) {
    scene_rmr_beat b = {0, 0, 0, ss->rpb, 0, SCENE_RMR_BASS_NONE};
    uint32_t lowest = NONE, bass = SCENE_SONG_VOICES;
    uint8_t ctrl[SCENE_SONG_VOICES];
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        const live_voice* x = &l->voice[v];
        ctrl[v] = live_view_voice_control(l, v);
        ss->gate[v] = (uint8_t)(x->gate && ctrl[v]);
        if (ss->gate[v] && !(ctrl[v] & 0x80u) && x->note < lowest) {
            lowest = x->note;
            bass = v;
        }
    }
    int sounding = 0;
    for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
        const live_voice* x = &l->voice[v];
        sounding |= ss->gate[v];
        if (ss->gate[v] && x->stamp != ss->stamp[v]) {
            if (ctrl[v] & 0x80u) {
                b.hits |= (uint8_t)hit_of(scene_song_drum_of_pitch(x->note));
            } else if (v == bass) {
                b.bass = bass_of(ss, x->note);
            } else {
                b.hits |= SCENE_RMR_ARP;
            }
        }
        ss->stamp[v] = x->stamp;
    }
    if (sounding && ++ss->free >= SCENE_SONG_FREE_FRAMES) {
        ss->free = 0;
        uint32_t s4 = ss->rows_seen ? (ss->s + 1u) % (4u * ss->rpb) : 0u;
        if (s4 == 0) {
            ss->bar = (uint8_t)((ss->bar + 1u) & 3u);
            new_bar(ss);
        }
        ss->rows_seen = 1;
        ss->s = (uint8_t)s4;
        b.row = 1;
        b.s = ss->s;
        b.bar = ss->bar;
    }
    if (b.row || b.hits || b.bass != SCENE_RMR_BASS_NONE) {
        push(ss, &b);
    }
    out->cutoff = l->cutoff_out;
    out->sounding = (uint8_t)sounding;
}

void scene_song_frame(scene_song* ss, const live* l, scene_song_frame_out* out) {
    if (ss->playing && ss->sc) {
        for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
            ss->stamp[v] = l->voice[v].stamp; /* the keys' notes are not the song's */
        }
        flush(ss);
        const uint8_t* reg = ss->sc->shadow;
        out->cutoff = (uint32_t)(reg[0x15] & 7u) | (uint32_t)reg[0x16] << 3;
        uint32_t quiet = (uint32_t)ss->sc->mute | ss->sc->taken;
        int sounding = 0;
        for (uint32_t v = 0; v < SCENE_SONG_VOICES; v++) {
            sounding |= ss->gate[v] && !(quiet >> v & 1u);
        }
        out->sounding = (uint8_t)sounding;
    } else {
        from_live(ss, l, out);
    }
    out->n = ss->n;
    for (uint32_t i = 0; i < ss->n; i++) {
        out->beat[i] = ss->beat[i];
    }
    ss->n = 0;
}
