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
#include "reg_ring.h"
#include "scope.h"
#include "tail_gate.h"
#include "thump_out.h"
#include "sid/sid.h"
#include "song/gt2_song.h"

/* The effect slots' seed (their noise and drift generators). */
#define SLOTS_SEED 0x51D0u

/* reg_ring_drain runs from SRAM (app.ld): beyond the direct call range of this file's
 * main-loop code in flash, so those calls go through a pointer the compiler cannot fold back
 * into a direct call (as Felucca's FL_FAR). audio_fill, in SRAM too, calls it directly. */
static uint32_t far_drain(reg_ring* r, reg_ring_chips* c) {
    uint32_t (*volatile fn)(reg_ring*, reg_ring_chips*) = reg_ring_drain;
    return fn(r, c);
}

/* Chips A and B and their shared spare (three sids, ~17 KB each, and three DAC tables), the
 * effect slots (~6.9 KB) and their 128 KiB delay/reverb pool: the big zero-initialised pool,
 * not .bss. */
static reg_ring_chips chips __attribute__((section(".pool")));
static sid_slots slots __attribute__((section(".pool")));
static sid_slot_pool slot_pool __attribute__((section(".pool")));
/* The slots' settings as live sent them; the drain applies them (in the interrupt). */
static fx_live fx;

static reg_ring ring;
static reg_ring_out out;
/* The writer bound to chip B (reg_ring_out.chip 1): keys_route's (app/keys_route.h), its
 * first reset at boot and the keys while they play it. */
static reg_ring_out out_b;
/* Chip B's users as the main loop has them (audio_chip_b_use: the keys, Task 7's UI sounds),
 * and chip B in use as the last block had it: the interrupt fades at the block where they
 * differ. */
static volatile uint8_t b_want;
static uint8_t b_on;
/* The MASTER gain (app/master_gain.h) as how far each is below unity, so full MASTER, the usual
 * case, is both 0 and the interrupt asks one question of it: master_to as the main loop last
 * set it, master_now the gain the last block ended on (the interrupt's). */
static volatile uint32_t master_to = 0;
static uint32_t master_now = 0;
/* The two chips' sum's excess past int16 in a block where every sum fits (app/chip_mix.h):
 * none. */
static int16_t no_over[AUDIO_HALF_FRAMES]; /* RAM: the interrupt reads it */
/* CHIP SPREAD (app/chip_spread.h): the main loop writes the gains (mid, side) it sets into the
 * pair the interrupt does not read, then points it there; the interrupt keeps the mid gain the
 * last block ended on and the side's with the slots' dry gain then. The stage runs while either
 * mid gain is not 1 (SPREAD 0). */
static float spread_gains[2][2] = {{1.0f, 0.0f}, {1.0f, 0.0f}};
static volatile uint8_t spread_pick;
static float spread_now[2] = {1.0f, 0.0f};
/* SPREAD's glide between the chips sounding alone (centred) and together (separated). */
static chip_spread_glide spread_glide;
/* The tail gate's state (app/tail_gate.h): the interrupt's. */
static tail_gate tail_state;
/* The chips' thump on the output (app/thump_out.h): the interrupt's. */
static thump_out thump;
/* The chip the tap reads (audio_tap_chip): 0 A, 1 B. */
static volatile uint8_t tap_chip;

/* The song (M2c Task 6). Its content region (decision record 0007): one 64 KiB block, 4 KiB
 * aligned, first in POOL (app.ld, __song_region), which M3's 6502 memory will overlay. The
 * player's state, its shadow and the rings (song_ctl) and the two shapes it reads are the
 * interrupt's: RAM. The main loop's work space (two analyses, a fast-forward player, the edits
 * in flight) is big and the main loop's only: POOL. */
static uint32_t song_region[SONG_REGION_BYTES / 4u]
    __attribute__((section(".pool.song_region"), aligned(4096)));
/* Not zeroed by the boot (app.ld keeps them out of the zeroed ranges): audio_song_boot sets
 * them up in the LCD's reset wait, so the song costs the boot's timeline nothing. The
 * interrupt's in the RAM region, the main loop's work space in POOL. */
static gt2_shape song_shapes[2] __attribute__((section(".song_noinit")));
static song_ctl song_transport __attribute__((section(".song_noinit")));
static song_work song_space __attribute__((section(".pool.song_noinit")));
/* The LIVE view's tap (app/scope.h): the voices every SCOPE_DECIMATE samples, the envelopes
 * and the output's peaks, while the view shows. */
static scope tap;
_Static_assert(AUDIO_HALF_FRAMES % SCOPE_DECIMATE == 0u, "a block is whole chunks of the tap");
static volatile uint8_t running;
volatile uint32_t audio_block_end;
/* The hold (audio_hold): wanted by the main loop; halves written silent since the fade (2:
 * held); the next rendered block fades in. */
static volatile uint8_t hold_want;
static volatile uint8_t hold_silent;
static uint8_t hold_faded;
static uint8_t fade_in;
/* The peak scan (audio_peak_scan) and the quiet blocks it counted. */
static volatile uint8_t peak_on;
static volatile uint32_t quiet_blocks;
/* Each block's anchor (audio_song_anchor): the tick its render started, its first sample; seq
 * odd while the interrupt writes them. */
static volatile uint32_t anchor_seq, anchor_ticks, anchor_samples;

/* midi.h's (which this file cannot include: its song_transport type is this file's variable). */
int midi_usb_live(void);

/* reg_ring_out.wait: the producer needs the interrupt to drain. Before audio_irqs_on the
 * interrupts are off, and an idle would never wake: drain here instead. */
static void wait_for_drain(void* ctx) {
    (void)ctx;
    if (!running) {
        far_drain(&ring, &chips);
        return;
    }
    /* While a USB host has the device configured, no `idle` (main.c wait_for_interrupt, and
     * docs/hardware-test.md section 8): back to the caller, whose loop spins until the
     * interrupt has made room. Else sleep until the next interrupt (TIMER5 within 100 us, the
     * block within 2.9 ms). */
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
    out.chip = 0; /* the instrument plays chip A until M2b Task 4 */
    out.wait = wait_for_drain;
    out.wait_ctx = 0;
    out_b = out;
    out_b.chip = 1;
    chips.swapped = song_ctl_chip_swapped; /* a swap of chip A while a song plays */
    chips.swapped_ctx = &song_transport;
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
    song_ctl_init(&song_transport, s, &song_shapes[0], &song_shapes[1], &song_space);
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
    out.sliced = 1; /* store_fm1_service skips its work in this pass */
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
    song_ctl_commit(&song_transport); /* the pass's song commands with its register writes */
    reg_ring_pass_end(&chips); /* one slice of a chip's preparation a pass */
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
        master_now = MASTER_UNITY - g; /* before the interrupt runs: no ramp, the first block has it */
    }
}

void audio_set_spread(uint32_t spread) {
    uint8_t j = (uint8_t)(spread_pick ^ 1u);
    chip_spread_gains(spread, &spread_gains[j][0], &spread_gains[j][1]);
    spread_pick = j; /* the stage glides to it once both chips sound (audio_fill) */
}

/* Part of chip A's block: with the LIVE view's tap (tapped) or without. */
static void render_part(sid* a, int16_t* s, uint32_t n, int tapped) {
    if (tapped) {
        scope_render(&tap, a, s, n);
    } else {
        sid_render(a, s, n);
    }
}

/* Chip A's block, split at the song's call, whose writes go straight into chip A
 * (song_ctl.h; decision record 0005's split render). */
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

/* Scales a block's samples by a ramp: down from full to 0 (down 1) or up from 0 (down 0).
 * In .ram_text with audio_fill (app.ld). */
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
    int16_t over[AUDIO_HALF_FRAMES]; /* the two chips' sum past int16 (app/chip_mix.h) */
    const int16_t* sum_over = 0;     /* over, or no_over while every sum fits */
    int16_t lr[2u * AUDIO_HALF_FRAMES];
    if (hold_want && hold_faded) {
        /* Held: silence, nothing rendered (the chips stand still). The ring still drains, so
         * a main loop waiting for room (wait_for_drain) goes on, and releases the hold. */
        reg_ring_drain(&ring, &chips);
        for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i++) {
            buf[i] = 0;
        }
        hold_silent = (uint8_t)(hold_silent < 2u ? hold_silent + 1u : 2u);
        quiet_blocks += peak_on;
        return;
    }
    reg_ring_drain(&ring, &chips);
    sid* a = reg_ring_chip_of(&chips, 0);
    sid* b = reg_ring_chip_of(&chips, 1);
    song_ctl_block(&song_transport, a);
    anchor_seq++;
    anchor_ticks = fm1_ticks();
    anchor_samples = song_transport.samples;
    anchor_seq++;
    /* Chip B is in use while it has a user, and after its last one let go until its three
     * envelopes are at 0 (the keys' release tails play out, and nothing is frozen to come
     * back later); only once its boot reset has made it (a zeroed sid has a sample rate of 0,
     * and must never render). */
    uint32_t tails = b_on ? chip_spread_level(b) : 0u;
    uint8_t want = (uint8_t)((b_want || tails) && b->sample_rate);
    const sid* b_rendered = 0;
    /* CHIP SPREAD (app/chip_spread.h): the separation glides in while both chips sound, so a
     * chip alone stays centred; at none the stage does not run (mono). Its side: chip A's own
     * output while B is out of use, else A - B halved (its gain doubled to match). */
    const float* spread_to = spread_gains[spread_pick];
    const float glide = (float)chip_spread_glide_step(&spread_glide, chip_spread_level(a), tails) *
                        (1.0f / (float)CHIP_SPREAD_GLIDE);
    const float mid_to = 1.0f + glide * (spread_to[0] - 1.0f);
    int spread = mid_to != 1.0f || spread_now[0] != 1.0f;
    int16_t side_ab[AUDIO_HALF_FRAMES];
    const int16_t* side = s;
    float side_w = 1.0f;
    if (!want && !b_on) {
        /* Chip A alone, as M1: sid_render's samples, and the tap's points while LIVE shows. */
        render_a(a, s, 1);
    } else {
        int16_t sb[AUDIO_HALF_FRAMES];
        if (tap_chip) {
            render_a(a, s, 0);
            scope_render(&tap, b, sb, AUDIO_HALF_FRAMES);
        } else {
            render_a(a, s, 1);
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
    /* The tail after notes (app/tail_gate.h): the chips' own leak muted once they have been done
     * a while. One compare while a note sounds. */
    if (!tail_gate_sounding(&tail_state, a)) {
        tail_gate_run(&tail_state, a, b_rendered, s);
        if (side != s) {
            tail_gate_also(&tail_state, side_ab);
        }
    }
    song_ctl_block_end(&song_transport, AUDIO_HALF_FRAMES);
    /* With every slot off this writes chip A's output to both sides unchanged; with chip B the
     * whole sum, through the soft limiter (core/fx/fx_limit.h) as the slots' output goes. The
     * tail gate leaves the sum's excess as it is: with both chips quiet there is none. CHIP
     * SPREAD takes the slots' output before its conversion and adds the side at the slots' dry
     * gain; at SPREAD 0 (mono) the slots alone. */
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
    /* The chips' thump, fitted into the room the slots' output leaves (app/thump_out.h): one
     * check a block while none sounds. A chip B that did not render was not heard. */
    thump_out_take(&thump, 0, a);
    if (b_rendered) {
        thump_out_take(&thump, 1, b);
    } else {
        thump_out_drop(b);
    }
    thump_out_apply(&thump, lr, AUDIO_HALF_FRAMES);
    /* MASTER: a digital gain on everything, after the chips and the slots. */
    uint32_t to = master_to;
    if ((to | master_now) != 0u) { /* full MASTER: no call at all */
        master_gain_apply(lr, MASTER_BLOCK_LOG2, MASTER_UNITY - master_now, MASTER_UNITY - to);
        master_now = to;
    }
    if (hold_want) {
        audio_ramp(lr, 1); /* the hold's fade out; silence from the next block */
        hold_faded = 1;
        fade_in = 1;
    } else if (fade_in) {
        audio_ramp(lr, 0);
        fade_in = 0;
        hold_faded = 0;
    }
    if (peak_on) {
        /* Every ninth value, L and R in turn: under 2 instructions a sample. */
        int32_t m = 0;
        for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i += 9u) {
            m |= lr[i] ^ (lr[i] >> 15);
        }
        quiet_blocks = m < AUDIO_QUIET_PEAK ? quiet_blocks + 1u : 0u;
    }
    scope_peaks(&tap, lr, AUDIO_HALF_FRAMES);
    /* Four words a pass: the loop's own instructions are most of what a word costs here. */
    for (uint32_t i = 0; i < 2u * AUDIO_HALF_FRAMES; i += 4u) {
        int32_t x0 = lr[i], x1 = lr[i + 1u], x2 = lr[i + 2u], x3 = lr[i + 3u];
        buf[i] = x0 * 256;
        buf[i + 1u] = x1 * 256;
        buf[i + 2u] = x2 * 256;
        buf[i + 3u] = x3 * 256;
    }
}
