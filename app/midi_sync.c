// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "midi_sync.h"

#include <stddef.h>

#define CLOCK 0xf8u
#define START 0xfau
#define CONTINUE 0xfbu
#define STOP 0xfcu
#define SPP 0xf2u
#define CIN_SINGLE 0x0fu
#define CIN_COMMON3 0x03u
#define SAMPLE_RATE 44118u
#define OUT_CATCH_UP 2u

static const char* const MODE_NAMES[MIDI_SYNC_MODES] = {"AUTO", "FOLLOW", "OFF"};

const char* midi_sync_mode_name(uint32_t mode) {
    return mode < MIDI_SYNC_MODES ? MODE_NAMES[mode] : "?";
}

void midi_sync_init(midi_sync* s) {
    uint8_t* p = (uint8_t*)s;
    for (uint32_t i = 0; i < sizeof *s; i++) {
        p[i] = 0;
    }
    midi_clock_init(&s->clock, SAMPLE_RATE);
    s->out_room = MIDI_SYNC_OUT_MAX;
    s->mode = MIDI_SYNC_AUTO;
    s->rows_per_beat = 4u;
}

void midi_sync_bind(midi_sync* s, const song_transport* t) {
    s->t = t;
}

void midi_sync_tune(midi_sync* s, midi_sync_tune_fn fn, void* ctx) {
    s->tune = fn;
    s->tune_ctx = ctx;
}

void midi_sync_song_changed(midi_sync* s) {
    s->want = 0;
    s->held = s->aligning = s->synced = s->seek_on = s->fresh = 0;
    s->subtune = 0;
    s->next_row = 0;
    s->out_state = MIDI_SYNC_IDLE;
    s->out_clock = 0;
}

static int bound(const midi_sync* s) {
    const song_transport* t = s->t;
    return t && t->hold && t->align && t->period && t->seek && t->seek_further && t->clock;
}

void midi_sync_set(midi_sync* s, uint32_t mode, uint32_t clock_out) {
    s->mode = (uint8_t)(mode < MIDI_SYNC_MODES ? mode : MIDI_SYNC_AUTO);
    s->clock_out = clock_out ? 1u : 0u;
    if (s->mode == MIDI_SYNC_OFF) {
        s->want = MIDI_SYNC_WANT_FREE;
        s->synced = 0;
    }
}

void midi_sync_rows(midi_sync* s, uint32_t rows_per_beat) {
    s->rows_per_beat = (uint8_t)(rows_per_beat ? rows_per_beat : 4u);
}

static int32_t samples_in(int32_t d) {
    return d >= 0 ? d / (int32_t)MIDI_SYNC_TICKS_PER_SAMPLE
                  : -((-d) / (int32_t)MIDI_SYNC_TICKS_PER_SAMPLE);
}

void midi_sync_anchor(midi_sync* s, uint32_t ticks, uint32_t samples) {
    if (!s->anchored) {
        s->r_ticks = ticks;
        s->r_base = samples;
    } else {
        uint32_t n = (ticks - s->r_ticks) / MIDI_SYNC_TICKS_PER_SAMPLE;
        s->r_base += n;
        s->r_ticks += n * MIDI_SYNC_TICKS_PER_SAMPLE;
    }
    s->a_ticks = ticks;
    s->a_samples = samples;
    s->anchored = 1;
}

uint32_t midi_sync_sample_of(const midi_sync* s, uint32_t ticks) {
    return s->a_samples + (uint32_t)samples_in((int32_t)(ticks - s->a_ticks)) -
           MIDI_SYNC_LATENCY;
}

static uint32_t real_of(const midi_sync* s, uint32_t ticks) {
    return s->r_base + (uint32_t)samples_in((int32_t)(ticks - s->r_ticks));
}

static uint32_t heard_of(const midi_sync* s, uint32_t real) {
    return real + midi_sync_sample_of(s, s->a_ticks) - real_of(s, s->a_ticks);
}

void midi_sync_held(midi_sync* s, uint32_t from, uint32_t to) {
    if (s->anchored) {
        midi_clock_hold(&s->clock, real_of(s, from), real_of(s, to));
    }
}

int midi_sync_following(const midi_sync* s) {
    return s->mode != MIDI_SYNC_OFF && s->clock.active;
}

__attribute__((noinline, unused)) static int tune_msg(midi_sync* s, const midi_msg* msg) {
    if (!s->tune(s->tune_ctx, MIDI_SYNC_TUNE_ASK)) {
        return 0;
    }
    uint32_t b = msg->data1;
    if (s->mode != MIDI_SYNC_OFF && msg->kind == MIDI_REALTIME &&
        (b == START || b == STOP || b == CONTINUE)) {
        (void)s->tune(s->tune_ctx, b);
    }
    return 1;
}

void midi_sync_msg(midi_sync* s, const midi_msg* msg, uint32_t ticks) {
    if (s->tune && tune_msg(s, msg)) {
        return;
    }
    if (msg->kind == MIDI_SONG_SELECT) {
        s->subtune = msg->data1;
        return;
    }
    if (s->mode == MIDI_SYNC_OFF || !s->anchored ||
        (msg->kind != MIDI_REALTIME && msg->kind != MIDI_SONG_POSITION)) {
        return;
    }
    midi_clock_event ev = midi_clock_msg(&s->clock, msg, real_of(s, ticks));
    if (msg->kind == MIDI_REALTIME && msg->data1 == CLOCK) {
        s->fresh = 1;
    }
    switch (ev) {
    case MIDI_CLOCK_EV_START:
        s->want = (uint8_t)((s->want & MIDI_SYNC_WANT_STOP) | MIDI_SYNC_WANT_START);
        s->synced = 0;
        break;
    case MIDI_CLOCK_EV_CONTINUE:
        s->want = (uint8_t)((s->want & ~MIDI_SYNC_WANT_STOP) | MIDI_SYNC_WANT_RESUME);
        break;
    case MIDI_CLOCK_EV_STOP:
        s->want = MIDI_SYNC_WANT_STOP;
        s->aligning = 0;
        break;
    case MIDI_CLOCK_EV_POSITION:
        s->want |= MIDI_SYNC_WANT_SEEK;
        s->seek_pos = s->clock.position;
        break;
    default: break;
    }
}

static uint32_t beat2(const midi_sync* s, const song_clock* st) {
    return (uint32_t)(st->speed[0] + st->speed[1]) * s->rows_per_beat;
}

static uint32_t call_of(const midi_sync* s, const song_clock* st, uint32_t pos) {
    return pos * beat2(s, st) / (2u * MIDI_CLOCK_PER_BEAT);
}

static int clock_time(const midi_sync* s, uint32_t whole, float frac, uint32_t* t) {
    uint32_t at;
    if (!midi_clock_time_of(&s->clock, whole, &at)) {
        return 0;
    }
    *t = heard_of(s, at + (uint32_t)(int32_t)(frac * s->clock.period + 0.5f));
    return 1;
}

static uint32_t horizon(const midi_sync* s) {
    return s->a_samples + MIDI_SYNC_LATENCY;
}

static void align_start(midi_sync* s, const song_clock* st) {
    uint32_t pos = s->p_anchor;
    uint32_t t;
    if (!clock_time(s, pos, 0.0f, &t)) {
        if (!s->clock.based || !s->fresh) {
            return;
        }
        pos = s->clock.position - 1u;
        t = heard_of(s, s->clock.last);
    }
    t += MIDI_SYNC_DELAY;
    if ((int32_t)(t - horizon(s)) < 0) {
        t = horizon(s);
    }
    if (!s->t->align(s->t->ctx, t)) {
        return;
    }
    s->aligns++;
    s->aligning = 0;
    s->held = 0;
    s->synced = 1;
    s->p_anchor = pos;
    s->next_row = s->c_anchor;
    (void)st;
}

static void follow(midi_sync* s, const song_clock* st) {
    uint32_t b2 = beat2(s, st);
    if (b2 == 0u || st->calls == 0u || st->calls < s->next_row) {
        return;
    }
    int32_t late = 0;
    uint32_t target = 0;
    int placed = 0;
    if (s->synced && st->calls >= s->c_anchor) {
        float dp = (float)(st->calls - s->c_anchor) * (float)(2u * MIDI_CLOCK_PER_BEAT) /
                   (float)b2;
        uint32_t whole = (uint32_t)dp;
        if (clock_time(s, s->p_anchor + whole, dp - (float)whole, &target)) {
            target += MIDI_SYNC_DELAY;
            uint32_t predicted = st->call_sample + (st->period_q16 >> 16);
            late = (int32_t)(predicted - target);
            placed = 1;
        }
    }
    if (placed && (late > (int32_t)MIDI_SYNC_ALIGN_SAMPLES ||
                   late < -(int32_t)MIDI_SYNC_ALIGN_SAMPLES)) {
        if ((int32_t)(target - horizon(s)) >= 0 && s->t->align(s->t->ctx, target)) {
            s->aligns++;
            late = 0;
        }
    }
    uint32_t p = midi_clock_frame_q16(&s->clock, st->speed[0], st->speed[1], s->rows_per_beat,
                                      late);
    if (p != 0u && s->t->period(s->t->ctx, p)) {
        s->periods++;
        s->next_row = st->calls + (st->speed[0] ? st->speed[0] : 1u);
    }
}

static int play(midi_sync* s, int held) {
    s->t->hold(s->t->ctx, (uint32_t)held);
    if (!s->t->play(s->t->ctx, 0u)) {
        s->t->hold(s->t->ctx, 0u);
        return 0;
    }
    s->held = (uint8_t)held;
    return 1;
}

static int resume(midi_sync* s, int held) {
    s->t->hold(s->t->ctx, (uint32_t)held);
    if (!s->t->resume(s->t->ctx)) {
        s->t->hold(s->t->ctx, 0u);
        return 0;
    }
    s->held = (uint8_t)held;
    return 1;
}

static void lost(midi_sync* s, const song_clock* st) {
    s->lost++;
    s->synced = 0;
    if (s->aligning && st->state == MIDI_SYNC_PLAYING) {
        if (s->t->align(s->t->ctx, horizon(s))) {
            s->aligning = 0;
            s->held = 0;
        }
    }
    if (s->mode == MIDI_SYNC_FOLLOW && st->state == MIDI_SYNC_PLAYING) {
        s->want = MIDI_SYNC_WANT_STOP;
    }
}

static void requests(midi_sync* s, const song_clock* st, int following) {
    if (s->want & MIDI_SYNC_WANT_STOP) {
        if (st->state == MIDI_SYNC_LOADING) {
            return;
        }
        if (st->state != MIDI_SYNC_PLAYING || s->t->stop(s->t->ctx)) {
            s->want &= (uint8_t)~MIDI_SYNC_WANT_STOP;
            s->synced = 0;
            s->aligning = 0;
            s->held = 0;
        }
        return;
    }
    if (s->want & MIDI_SYNC_WANT_START) {
        if (st->state == MIDI_SYNC_PLAYING) {
            s->want |= MIDI_SYNC_WANT_STOP;
            return;
        }
        if (st->state == MIDI_SYNC_STOPPED) {
            s->want = (uint8_t)((s->want & ~MIDI_SYNC_WANT_START) | MIDI_SYNC_WANT_SEEK |
                                MIDI_SYNC_WANT_RESUME);
            s->seek_pos = 0;
        } else if (st->state == MIDI_SYNC_IDLE &&
                   play(s, following)) {
            s->want &= (uint8_t)~MIDI_SYNC_WANT_START;
            s->aligning = (uint8_t)following;
            s->c_anchor = 0;
            s->p_anchor = 0;
            s->seek_on = 0;
            s->fresh = 0;
            return;
        } else {
            return;
        }
    }
    if ((s->want & MIDI_SYNC_WANT_SEEK) && st->state == MIDI_SYNC_PLAYING && !s->aligning) {
        s->want &= (uint8_t)~MIDI_SYNC_WANT_SEEK;
    }
    if ((s->want & MIDI_SYNC_WANT_SEEK) && !s->seek_on) {
        if (st->state == MIDI_SYNC_IDLE || st->state == MIDI_SYNC_LOADING) {
            if (s->want & MIDI_SYNC_WANT_RESUME) {
                if (st->state == MIDI_SYNC_IDLE &&
                    play(s, following)) {
                    s->aligning = 0;
                }
            }
            return;
        }
        if (st->speed[0] == 0u) {
            return;
        }
        uint32_t call = call_of(s, st, s->seek_pos);
        if (s->t->seek(s->t->ctx, call)) {
            s->want &= (uint8_t)~MIDI_SYNC_WANT_SEEK;
            s->seek_on = 1;
            s->seek_call = call;
        }
        return;
    }
    if (s->want & MIDI_SYNC_WANT_RESUME) {
        if (s->seek_on && st->seeking == 1u) {
            if (s->clock.running) {
                uint32_t call = call_of(s, st, s->clock.position);
                if (call > s->seek_call && s->t->seek_further(s->t->ctx, call)) {
                    s->seek_call = call;
                }
            }
            return;
        }
        int resumed = 0;
        if (st->state == MIDI_SYNC_STOPPED) {
            resumed = resume(s, following);
        } else if (st->state == MIDI_SYNC_PLAYING) {
            resumed = 1;
        } else if (st->state == MIDI_SYNC_IDLE && !s->seek_on) {
            if (play(s, following)) {
                s->want &= (uint8_t)~MIDI_SYNC_WANT_RESUME;
                s->aligning = (uint8_t)following;
                s->c_anchor = 0;
                s->p_anchor = s->clock.position;
                s->fresh = 0;
            }
            return;
        }
        if (resumed) {
            s->want &= (uint8_t)~MIDI_SYNC_WANT_RESUME;
            s->aligning = (uint8_t)following;
            s->c_anchor = s->seek_on ? s->seek_call : st->calls;
            s->p_anchor = s->clock.position;
            s->seek_on = 0;
            s->fresh = 0;
        }
    }
}

static uint32_t realtime(uint8_t b) {
    return MIDI_SYNC_CABLE << 4 | CIN_SINGLE | (uint32_t)b << 8;
}

static uint32_t out_packets(midi_sync* s, const song_clock* st, uint32_t now,
                            uint32_t* out) {
    uint32_t n = 0;
    uint32_t room = s->out_room < MIDI_SYNC_OUT_MAX ? s->out_room : MIDI_SYNC_OUT_MAX;
    int on = s->clock_out && !midi_sync_following(s);
    uint32_t b2 = beat2(s, st);
    if (st->state != s->out_state && st->state != MIDI_SYNC_LOADING) {
        uint8_t was = s->out_state;
        int spp = on && st->state == MIDI_SYNC_PLAYING && was == MIDI_SYNC_STOPPED && b2 &&
                  !st->from_top && st->calls > 1u;
        if (spp && s->out_common_wait) {
            return n;
        }
        int sends = on && (st->state == MIDI_SYNC_PLAYING ||
                           (was == MIDI_SYNC_PLAYING && st->state == MIDI_SYNC_STOPPED));
        if (sends && room < (spp ? 2u : 1u)) {
            return n;
        }
        s->out_state = st->state;
        if (on && st->state == MIDI_SYNC_PLAYING) {
            if (spp) {
                uint32_t clocks = st->calls * (2u * MIDI_CLOCK_PER_BEAT) / b2;
                uint32_t pos = clocks / MIDI_CLOCK_PER_16TH;
                s->out_clock = pos * MIDI_CLOCK_PER_16TH;
                out[n++] = MIDI_SYNC_CABLE << 4 | CIN_COMMON3 | SPP << 8 | (pos & 0x7fu) << 16 |
                           ((pos >> 7) & 0x7fu) << 24;
                out[n++] = realtime(CONTINUE);
            } else {
                s->out_clock = 0;
                out[n++] = realtime(START);
            }
        } else if (on && was == MIDI_SYNC_PLAYING && st->state == MIDI_SYNC_STOPPED) {
            out[n++] = realtime(STOP);
        }
    }
    if (!on || st->state != MIDI_SYNC_PLAYING || b2 == 0u || st->calls == 0u) {
        return n;
    }
    float period = (float)st->period_q16 * (1.0f / 65536.0f);
    float calls_a_clock = (float)b2 / (float)(2u * MIDI_CLOCK_PER_BEAT);
    float per = calls_a_clock * period;
    while (n < room) {
        float d = (float)s->out_clock * calls_a_clock - (float)(st->calls - 1u);
        uint32_t at = st->call_sample + (uint32_t)(int32_t)(d * period);
        int32_t wait = (int32_t)(at - now);
        if (wait > 0) {
            break;
        }
        if (per >= 1.0f && (float)(-wait) > (float)OUT_CATCH_UP * per) {
            s->out_clock += (uint32_t)((float)(-wait) / per);
            continue;
        }
        out[n++] = realtime(CLOCK);
        s->out_clock++;
    }
    return n;
}

uint32_t midi_sync_poll(midi_sync* s, uint32_t ticks, uint32_t out[MIDI_SYNC_OUT_MAX]) {
    if (!bound(s) || !s->anchored) {
        return 0;
    }
    song_clock st;
    s->t->clock(s->t->ctx, &st);
    uint32_t now = midi_sync_sample_of(s, ticks);
    if (s->want & MIDI_SYNC_WANT_FREE) {
        s->t->hold(s->t->ctx, 0u);
        if (st.state == MIDI_SYNC_LOADING) {
            return 0;
        }
        if (s->held) {
            if (st.state == MIDI_SYNC_PLAYING && !s->t->align(s->t->ctx, horizon(s))) {
                return 0;
            }
            s->aligning = 0;
            s->held = 0;
        }
        if (!s->t->period(s->t->ctx, 0u)) {
            return 0;
        }
        s->want &= (uint8_t)~MIDI_SYNC_WANT_FREE;
    }
    if (s->mode != MIDI_SYNC_OFF &&
        midi_clock_poll(&s->clock, real_of(s, ticks)) == MIDI_CLOCK_EV_LOST) {
        lost(s, &st);
    }
    int following = midi_sync_following(s);
    if (s->mode != MIDI_SYNC_OFF) {
        requests(s, &st, following);
        s->t->clock(s->t->ctx, &st);
        if (s->aligning && (st.state == MIDI_SYNC_IDLE || st.state == MIDI_SYNC_STOPPED) &&
            !(s->want & (MIDI_SYNC_WANT_RESUME | MIDI_SYNC_WANT_SEEK))) {
            s->t->hold(s->t->ctx, 0u);
            s->aligning = 0;
            s->held = 0;
        }
        if (s->aligning && st.state == MIDI_SYNC_PLAYING && st.seeking != 1u) {
            align_start(s, &st);
        } else if (following && st.state == MIDI_SYNC_PLAYING && !s->aligning) {
            follow(s, &st);
        }
    }
    s->fresh = 0;
    uint32_t n = out_packets(s, &st, now, out);
    s->sent += n;
    return n;
}
