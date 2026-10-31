// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "audio.h"

#include "fm1_time.h"

#include "chip_mix.h"
#include "chip_spread.h"
#include "fx/fx_slots.h"
#include "fx_live.h"
#include "live.h"
#include "master_gain.h"
#include "player/song_ctl.h"
#include "psid/psid_ctl.h"
#include "reg_ring.h"
#include "scope.h"
#include "tail_gate.h"
#include "thump_out.h"
#include "sid/sid.h"
#include "song/gt2_song.h"

#define SLOTS_SEED 0x51D0u

static uint32_t far_drain(reg_ring* r, reg_ring_chips* c) {
    uint32_t (*volatile fn)(reg_ring*, reg_ring_chips*) = reg_ring_drain;
    return fn(r, c);
}

static reg_ring_chips chips __attribute__((section(".pool")));
static sid_slots slots __attribute__((section(".pool")));
static sid_slot_pool slot_pool __attribute__((section(".pool")));
static fx_live fx;

static reg_ring ring;
static reg_ring_out out;
static reg_ring_out out_b;
static volatile uint8_t b_want;
static uint8_t b_on;
static volatile uint32_t master_to = 0;
static uint32_t master_now = 0;
static int16_t no_over[AUDIO_HALF_FRAMES];
static float spread_gains[2][2] = {{1.0f, 0.0f}, {1.0f, 0.0f}};
static volatile uint8_t spread_pick;
static float spread_now[2] = {1.0f, 0.0f};
static chip_spread_glide spread_glide;
static tail_gate tail_state;
static thump_out thump;
static volatile uint8_t tap_chip;

static uint32_t song_region[SONG_REGION_BYTES / 4u]
    __attribute__((section(".pool.song_region"), aligned(4096)));
_Static_assert(SONG_REGION_BYTES == 0x10000u, "the song's region is the 6502's 64 KiB");
static union {
    struct {
        song_ctl transport;
        gt2_shape shapes[2];
    } song;
    struct {
        psid_ctl ctl;
        psid_work work;
        psid_header header;
    } psid;
} player_isr __attribute__((section(".song_noinit")));
static union {
    song_work song;
    psid_lift lift;
} song_space __attribute__((section(".pool.song_noinit")));
_Static_assert(sizeof(psid_lift) <= sizeof(song_work), "the lift fits the song's work space");
_Static_assert(sizeof player_isr.psid <= sizeof player_isr.song,
               "a tune's state fits the song transport's and its shapes'");
#define song_transport (player_isr.song.transport)
#define song_shapes (player_isr.song.shapes)
#define tune_ctl (player_isr.psid.ctl)
__asm__(".set song_transport, player_isr");
static volatile uint8_t player_want = AUDIO_SONG;
static volatile uint8_t player_now = AUDIO_SONG;
static scope tap;
_Static_assert(AUDIO_HALF_FRAMES % SCOPE_DECIMATE == 0u, "a block is whole chunks of the tap");
static volatile uint8_t running;
volatile uint32_t audio_block_end;
static volatile uint8_t hold_want;
static volatile uint8_t hold_silent;
static uint8_t hold_faded;
static uint8_t fade_in;
static volatile uint8_t peak_on;
static volatile uint32_t quiet_blocks;
static volatile uint32_t anchor_seq, anchor_ticks, anchor_samples;

int midi_usb_live(void);

static void wait_for_drain(void* ctx) {
    (void)ctx;
    if (!running) {
        far_drain(&ring, &chips);
        return;
    }
    if (midi_usb_live()) {
        return;
    }
    __asm__ volatile("idle" ::: "memory");
}

void audio_init(void) {
    reg_ring_init(&ring);
    scope_init(&tap);
    reg_ring_chips_init(&chips, SID_MODEL_6581, SID_CLOCK_PAL, AUDIO_SAMPLE_RATE);
    sid_slots_init(&slots, AUDIO_SAMPLE_RATE, SLOTS_SEED, &slot_pool);
    fx_live_init(&fx, &slots);
    chips.pseudo = fx_live_pseudo;
    chips.pseudo_ctx = &fx;
    out.ring = &ring;
    out.chips = &chips;
    out.chip = 0;
    out.wait = wait_for_drain;
    out.wait_ctx = 0;
    out_b = out;
    out_b.chip = 1;
    chips.swapped = song_ctl_chip_swapped;
    chips.swapped_ctx = &song_transport;
}

void audio_player(uint32_t player) {
    player_want = (uint8_t)player;
}

uint32_t audio_player_now(void) {
    return player_now;
}

static inline sid* chip_now(uint32_t k) {
    return &chips.chip[chips.active[k & 1u]];
}

static sid* psid_hook_chip(void* ctx, uint32_t chip) {
    (void)ctx;
    return chip_now(chip);
}

static void psid_hook_mute(void* ctx, uint32_t chip, uint32_t mask) {
    (void)ctx;
    reg_ring_sink(chip ? &out_b : &out, REG_RING_MUTE, (uint8_t)mask);
}

psid_ctl* audio_psid_init(void) {
    psid_ctl_hooks hooks = {psid_hook_chip, psid_hook_mute, 0};
    psid_ctl_init(&tune_ctl, &player_isr.psid.work, (uint8_t*)song_region, &hooks);
    return &tune_ctl;
}

psid_ctl* audio_psid(void) {
    return &tune_ctl;
}

psid_lift* audio_lift(void) {
    return &song_space.lift;
}

psid_header* audio_psid_header(void) {
    return &player_isr.psid.header;
}

int audio_psid_poll(void) {
    if (chips.prep_pass || chips.build_pass) {
        return 0;
    }
    int ran = psid_ctl_poll(&tune_ctl);
    if (ran) {
        chips.prep_pass = 1;
        out.sliced = 1;
    }
    return ran;
}

sid* audio_chip(uint32_t chip) {
    return chip_now(chip);
}

void* audio_sink_ctx_b(void) {
    return &out_b;
}

void audio_keys(void* ctx, uint32_t chip) {
    (void)ctx;
    reg_ring_set_keys(&chips, chip);
    audio_chip_b_use(AUDIO_B_KEYS, chip == 1u);
    audio_tap_chip(chip);
}

void audio_chip_b_use(uint32_t user, int on) {
    uint8_t w = b_want;
    b_want = (uint8_t)(on ? w | user : w & ~user);
}

void audio_tap_chip(uint32_t chip) {
    tap_chip = chip ? 1u : 0u;
}

gt2_song* audio_song_boot(void) {
    gt2_song* s = gt2_song_init(song_region, SONG_REGION_BYTES);
    song_ctl_init(&song_transport, s, &song_shapes[0], &song_shapes[1], &song_space.song);
    return s;
}

gt2_song* audio_song_remake(void) {
    gt2_song* s = gt2_song_init_wiped(song_region, SONG_REGION_BYTES);
    song_ctl_init(&song_transport, s, &song_shapes[0], &song_shapes[1], &song_space.song);
    return s;
}

song_ctl* audio_song(void) {
    return &song_transport;
}

void audio_song_anchor(uint32_t* ticks, uint32_t* samples) {
    uint32_t seq;
    do {
        seq = anchor_seq;
        *ticks = anchor_ticks;
        *samples = anchor_samples;
    } while ((seq & 1u) || seq != anchor_seq);
}

void audio_song_poll(void) {
    uint32_t slices = song_transport.slices;
    song_ctl_poll(&song_transport,
                  (chips.prep_pass || chips.build_pass ? 0u : SONG_POLL_SLICE) | SONG_POLL_RING);
    if (song_transport.slices != slices) {
        chips.prep_pass = 1;
        out.sliced = 1;
    }
}

int audio_work_slice(void) {
    if (chips.prep_pass) {
        return 0;
    }
    out.sliced = 1;
    return 1;
}

scope* audio_scope(void) {
    return &tap;
}

void* audio_sink_ctx(void) {
    return &out;
}

void audio_commit(void) {
    reg_ring_commit(&ring);
    if (player_want == AUDIO_SONG && player_now == AUDIO_SONG) {
        song_ctl_commit(&song_transport);
    }
    reg_ring_pass_end(&chips);
}

uint32_t audio_queued(void) {
    return reg_ring_used(&ring);
}

void audio_start(void) {
    reg_ring_commit(&ring);
    far_drain(&ring, &chips);
}

void audio_irqs_on(void) {
    running = 1;
}

void audio_halted(void) {
    running = 0;
}

int audio_running(void) {
    return running;
}

void audio_set_master(uint32_t master) {
    uint32_t g = master_gain_of(master, LIVE_MASTER_MAX);
    master_to = MASTER_UNITY - g;
    if (!running) {
        master_now = MASTER_UNITY - g;
    }
}

void audio_set_spread(uint32_t spread) {
    uint8_t j = (uint8_t)(spread_pick ^ 1u);
    chip_spread_gains(spread, &spread_gains[j][0], &spread_gains[j][1]);
    spread_pick = j;
}

static void render_part(sid* a, int16_t* s, uint32_t n, int tapped) {
    if (tapped) {
        scope_render(&tap, a, s, n);
    } else {
        sid_render(a, s, n);
    }
}

__attribute__((minsize)) static void psid_rewrite(sid* to, const sid* from) {
    uint8_t r[25];
    for (uint32_t v = 0; v < 3u; v++) {
        const sid_osc* o = &from->osc[v];
        const sid_env* e = &from->env[v];
        uint8_t* q = &r[7u * v];
        q[0] = (uint8_t)o->freq;
        q[1] = (uint8_t)(o->freq >> 8);
        q[2] = (uint8_t)o->pw;
        q[3] = (uint8_t)(o->pw >> 8);
        q[4] = o->control;
        q[5] = (uint8_t)(e->attack << 4 | e->decay);
        q[6] = (uint8_t)((e->sustain & 0xf0u) | e->release);
    }
    r[21] = (uint8_t)(from->fc & 7u);
    r[22] = (uint8_t)(from->fc >> 3);
    r[23] = (uint8_t)(from->res << 4 | from->routing);
    r[24] = from->mode_vol;
    for (uint32_t i = 0; i < 25u; i++) {
        sid_write(to, (uint8_t)i, r[i]);
    }
}

__attribute__((minsize)) static void psid_swapped(void* ctx, uint32_t chip, sid* s) {
    const psid_ctl* c = ctx;
    if (c->mode == PSID_ISR_OFF || (chip && c->chips != 2u)) {
        return;
    }
    const sid* was = &chips.chip[3u - chips.active[0] - chips.active[1]];
    s->cycles_per_sample_q16 = was->cycles_per_sample_q16;
    psid_rewrite(s, was);
}

static void render_a(sid* a, int16_t* s, int tapped) {
    uint32_t at = song_ctl_due(&song_transport, AUDIO_HALF_FRAMES);
    if (at < AUDIO_HALF_FRAMES) {
        if (at > 0u) {
            render_part(a, s, at, tapped);
        }
        song_ctl_call(&song_transport);
        render_part(a, s + at, AUDIO_HALF_FRAMES - at, tapped);
    } else {
        render_part(a, s, AUDIO_HALF_FRAMES, tapped);
    }
}

__attribute__((noinline)) static void play_a(uint32_t player, sid* a, sid* b, int16_t* s,
                                             int16_t* sb, uint32_t tapped) {
    if (player == AUDIO_PSID) {
        (void)psid_ctl_block(&tune_ctl, a, b, s, sb, tapped, render_part);
    } else if (player == AUDIO_SONG) {
        render_a(a, s, (int)(tapped & 1u));
    } else {
        render_part(a, s, AUDIO_HALF_FRAMES, (int)(tapped & 1u));
    }
}

void audio_hold(int on) {
    if (!on) {
        hold_want = 0;
        return;
    }
    hold_silent = 0;
    hold_want = 1;
}

int audio_held(void) {
    return hold_want && hold_silent >= 2u;
}

void audio_peak_scan(int on) {
    if (!on) {
        quiet_blocks = 0;
    }
    peak_on = (uint8_t)(on != 0);
}

uint32_t audio_quiet_blocks(void) {
    return quiet_blocks;
}

int audio_pass_sliced(void) {
    int s = out.sliced | out_b.sliced;
    out.sliced = 0;
    out_b.sliced = 0;
    return s;
}

_Static_assert(AUDIO_HALF_FRAMES == 128u, "audio_ramp's shift");
static void audio_ramp(int16_t* lr, int down) {
    for (uint32_t f = 0; f < AUDIO_HALF_FRAMES; f++) {
        int32_t k = (int32_t)(down ? AUDIO_HALF_FRAMES - 1u - f : f);
        lr[2u * f] = (int16_t)((lr[2u * f] * k) >> 7);
        lr[2u * f + 1u] = (int16_t)((lr[2u * f + 1u] * k) >> 7);
    }
}

void audio_fill(int32_t* buf) {
    int16_t s[AUDIO_HALF_FRAMES];
    int16_t over[AUDIO_HALF_FRAMES];
    const int16_t* sum_over = 0;
    int16_t lr[2u * AUDIO_HALF_FRAMES];
    if (hold_want && hold_faded) {
        reg_ring_drain(&ring, &chips);
        for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i++) {
            buf[i] = 0;
        }
        hold_silent = (uint8_t)(hold_silent < 2u ? hold_silent + 1u : 2u);
        quiet_blocks += peak_on;
        return;
    }
    uint8_t player = player_now;
    if (player_want != player) {
        player = player_want;
        player_now = player;
        chips.swapped = player == AUDIO_SONG ? song_ctl_chip_swapped :
                        player == AUDIO_PSID ? psid_swapped : 0;
        chips.swapped_ctx = player == AUDIO_SONG ? (void*)&song_transport : (void*)&tune_ctl;
    }
    reg_ring_drain(&ring, &chips);
    sid* a = reg_ring_chip_of(&chips, 0);
    sid* b = reg_ring_chip_of(&chips, 1);
    const int song = player == AUDIO_SONG;
    const int two = player == AUDIO_PSID && tune_ctl.chips == 2u;
    if (song) {
        song_ctl_block(&song_transport, a);
        anchor_seq++;
        anchor_ticks = fm1_ticks();
        anchor_samples = song_transport.samples;
        anchor_seq++;
    }
    uint32_t tails = b_on ? chip_spread_level(b) : 0u;
    uint8_t want = (uint8_t)((b_want || tails) && b->sample_rate);
    const sid* b_rendered = 0;
    const float* spread_to = spread_gains[spread_pick];
    const float glide = (float)chip_spread_glide_step(&spread_glide, chip_spread_level(a), tails) *
                        (1.0f / (float)CHIP_SPREAD_GLIDE);
    const float mid_to = 1.0f + glide * (spread_to[0] - 1.0f);
    int spread = mid_to != 1.0f || spread_now[0] != 1.0f;
    int16_t side_ab[AUDIO_HALF_FRAMES];
    const int16_t* side = s;
    float side_w = 1.0f;
    if (!want && !b_on) {
        play_a(player, a, 0, s, 0, 1u);
    } else {
        int16_t sb[AUDIO_HALF_FRAMES];
        if (two) {
            play_a(player, a, b, s, sb, tap_chip ? 2u : 1u);
        } else if (tap_chip) {
            play_a(player, a, 0, s, 0, 0u);
            scope_render(&tap, b, sb, AUDIO_HALF_FRAMES);
        } else {
            play_a(player, a, 0, s, 0, 1u);
            sid_render(b, sb, AUDIO_HALF_FRAMES);
        }
        chip_mix_fade fade = want == b_on ? CHIP_MIX_FULL : want ? CHIP_MIX_IN : CHIP_MIX_OUT;
        if (spread) {
            chip_spread_side(side_ab, s, sb, fade);
            side = side_ab;
            side_w = 2.0f;
        }
        sum_over = chip_mix(s, sb, over, AUDIO_HALF_FRAMES, fade) ? over : no_over;
        b_on = want;
        b_rendered = b;
    }
    if (!tail_gate_sounding(&tail_state, a)) {
        tail_gate_run(&tail_state, a, b_rendered, s);
        if (side != s) {
            tail_gate_also(&tail_state, side_ab);
        }
    }
    if (song) {
        song_ctl_block_end(&song_transport, s, AUDIO_HALF_FRAMES);
    }
    if (spread) {
        float lrf[2u * AUDIO_HALF_FRAMES];
        sid_slots_process_float(&slots, s, sum_over, lrf, AUDIO_HALF_FRAMES);
        float k = glide * spread_to[1] * sid_slots_dry(&slots);
        chip_spread_apply(lr, lrf, side, spread_now[0], mid_to, spread_now[1] * side_w,
                          k * side_w);
        spread_now[0] = mid_to;
        spread_now[1] = k;
    } else {
        sid_slots_process_wide(&slots, s, sum_over, lr, AUDIO_HALF_FRAMES);
    }
    thump_out_take(&thump, 0, a);
    if (b_rendered) {
        thump_out_take(&thump, 1, b);
    } else {
        thump_out_drop(b);
    }
    thump_out_apply(&thump, lr, AUDIO_HALF_FRAMES);
    uint32_t to = master_to;
    if ((to | master_now) != 0u) {
        master_gain_apply(lr, MASTER_BLOCK_LOG2, MASTER_UNITY - master_now, MASTER_UNITY - to);
        master_now = to;
    }
    if (hold_want) {
        audio_ramp(lr, 1);
        hold_faded = 1;
        fade_in = 1;
    } else if (fade_in) {
        audio_ramp(lr, 0);
        fade_in = 0;
        hold_faded = 0;
    }
    if (peak_on) {
        int32_t m = 0;
        for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i += 9u) {
            m |= lr[i] ^ (lr[i] >> 15);
        }
        quiet_blocks = m < AUDIO_QUIET_PEAK ? quiet_blocks + 1u : 0u;
    }
    scope_peaks(&tap, lr, AUDIO_HALF_FRAMES);
    for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i += 4u) {
        int32_t x0 = lr[i], x1 = lr[i + 1u], x2 = lr[i + 2u], x3 = lr[i + 3u];
        buf[i] = x0 * 256;
        buf[i + 1u] = x1 * 256;
        buf[i + 2u] = x2 * 256;
        buf[i + 3u] = x3 * 256;
    }
}
