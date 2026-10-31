// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-FileCopyrightText: Lasse Öörni and GoatTracker 2's contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#ifndef SIDOU_GT2_PLAY_H
#define SIDOU_GT2_PLAY_H

#include <stdint.h>

#include "song/gt2_song.h"
#include "song/song_ext.h"

#ifdef __cplusplus
extern "C" {
#endif

#define GT2_PLAY_CHANNELS 3u
#define GT2_SID_REGS 25u
#define GT2_ORDER_STREAM 256u
#define GT2_VAR_MAX (GT2_SID_REGS + 5u * 7u * GT2_PLAY_CHANNELS)

typedef void (*gt2_sid_fn)(void* ctx, uint8_t reg, uint8_t value);

typedef struct {
    uint8_t songptr;
    uint8_t trans, repeat;
    uint8_t pattptr;
    uint8_t newfx, newparam, fx, param, newnote;
    uint8_t waveptr, wave, pulseptr, pulsetime;
    uint8_t pattnum, tempo, counter, note, instr, gate;
    uint8_t vibtime, vibdelay, wavetime, freqlo, freqhi, pulselo, pulsehi;
    uint8_t ad, sr, gatetimer, lastnote;
    uint8_t orderpos;
    uint8_t ev_pending, ev_pattern, ev_row, ev_orderpos;
    uint16_t skip[2];
    uint16_t encoding;
} gt2_channel;

enum {
    GT2_EV_ROW = 1u << 0,
    GT2_EV_NOTE = 1u << 1,
    GT2_EV_SLIDE = 1u << 2,
    GT2_EV_GATE_ON = 1u << 3,
    GT2_EV_GATE_OFF = 1u << 4
};

typedef struct {
    uint8_t flags;
    uint8_t pattern, row;
    uint8_t order_pos;
    uint8_t note, instr;
    uint8_t gate;
    uint8_t reserved;
} gt2_event;

enum {
    GT2_PF_BUFFERED = 1u << 0,
    GT2_PF_GHOST = 1u << 1,
    GT2_PF_ZPGHOST = 1u << 2,
    GT2_PF_ALT = 1u << 3,
    GT2_PF_PULSEOPT = 1u << 4,
    GT2_PF_REALTIMEOPT = 1u << 5,
    GT2_PF_AUTHORINFO = 1u << 6,
    GT2_PF_NOEFFECTS = 1u << 7,
    GT2_PF_NOFILTER = 1u << 8,
    GT2_PF_SIMPLEPULSE = 1u << 9,
    GT2_PF_WAVEDELAY = 1u << 10,
    GT2_PF_WAVECMD = 1u << 11
};

enum {
    GT2_LAYOUT_NORMAL = 1u << 0,
    GT2_LAYOUT_G4 = 1u << 1,
    GT2_LAYOUT_G5 = 1u << 2,
    GT2_LAYOUT_FIXEDPARAMS = 1u << 3,
    GT2_LAYOUT_NOINSVIB = 1u << 4,
    GT2_LAYOUT_NOCALCSPEED = 1u << 5
};

#define GT2_TICK0_TEMPO_GLOBAL (1u << 16)
#define GT2_TICK0_TEMPO_CHANNEL (1u << 17)

enum {
    GT2_PLAY_WARN_PITCH = 1u << 0
};

#define GT2_VAR_ZERO 0xfeu
#define GT2_VAR_UNKNOWN 0xffu
#define GT2_VAR_GHOST 0x80u
enum {
    GT2_VF_ORDIDX, GT2_VF_PATTNUM, GT2_VF_INSTR, GT2_VF_TEMPO, GT2_VF_COUNTER, GT2_VF_NOTE,
    GT2_VF_GATE, GT2_VF_VIBTIME, GT2_VF_VIBDELAY, GT2_VF_WAVETIME, GT2_VF_FREQLO,
    GT2_VF_FREQHI, GT2_VF_PULSELO, GT2_VF_PULSEHI, GT2_VF_AD, GT2_VF_SR, GT2_VF_GATETIMER,
    GT2_VF_LASTNOTE, GT2_VF_PULSETIME
};

typedef struct {
    uint16_t flags;
    uint8_t warnings;
    uint8_t subtune;
    uint8_t songs;
    uint8_t channels;
    uint8_t ad_param, sr_param;
    uint8_t reset_instr;
    uint8_t first_legato;
    uint8_t default_tempo;
    uint8_t first_note, last_note;
    uint8_t layout;
    uint8_t var_count;
    uint8_t reserved;
    uint32_t tick0;
    uint8_t var[GT2_VAR_MAX];
    uint8_t pat_num[GT2_MAX_PATTERNS];
    uint8_t ins_num[GT2_MAX_INSTR + 1u];
    uint8_t ord[GT2_PLAY_CHANNELS][GT2_ORDER_STREAM];
    uint8_t ord_pos[GT2_PLAY_CHANNELS][GT2_ORDER_STREAM];
} gt2_shape;
#define GT2_ORD_RAW 0xfeu

#define GT2_DRAFT_ROWS 10u
#define GT2_DRAFT_OFF 0xffu
typedef struct {
    _Alignas(4) uint8_t channel;
    uint8_t first, len, pad;
    uint8_t rows[2u * GT2_DRAFT_ROWS];
} gt2_draft;

typedef struct {
    const gt2_song* song;
    const gt2_shape* shape;
    uint8_t initsong;
    uint8_t filtstep, filttime, filtcutoff, filtctrl, filttype, masterfader;
    uint8_t funktempo[2];
    uint8_t timing_mark;
    uint8_t freq_unknown;
    uint8_t unknown;
    uint8_t gates;
    uint8_t frozen;
    uint8_t jam_voices;
    uint8_t draft_jam;
    uint8_t start[GT2_PLAY_CHANNELS];
    uint8_t regs[GT2_SID_REGS];
    gt2_channel chn[GT2_PLAY_CHANNELS];
    gt2_event ev[GT2_PLAY_CHANNELS];
    const gt2_draft* draft;
} gt2_player;

#define GT2_NOTES 96u
extern const uint8_t gt2_freq_lo[GT2_NOTES];
extern const uint8_t gt2_freq_hi[GT2_NOTES];

void gt2_play_start(gt2_player* p, const gt2_song* s, const gt2_shape* shape,
                    const uint8_t* start);

void gt2_play_set_shape(gt2_player* p, const gt2_shape* shape);
void gt2_play_retain_shape(gt2_player* p, const gt2_shape* shape);

void gt2_play_frame(gt2_player* p, gt2_sid_fn sink, void* ctx);

void gt2_play_jam(gt2_player* p, uint32_t ch, uint8_t note, uint8_t instr, gt2_sid_fn sink,
                  void* ctx);

void gt2_play_draft(gt2_player* p, const gt2_draft* d);

#ifdef __cplusplus
}
#endif

#endif
