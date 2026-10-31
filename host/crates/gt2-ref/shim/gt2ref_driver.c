/* SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#include <stdio.h>
#include <stdlib.h>

#include "gt2ref.h"

int main(int argc, char** argv) {
    if (argc < 3)
        return 2;
    FILE* f = fopen(argv[1], "rb");
    if (f == NULL)
        return 2;
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    unsigned char* d = malloc((size_t)n + 1);
    if (d == NULL || fread(d, 1, (size_t)n, f) != (size_t)n)
        return 2;
    fclose(f);
    gt2ref_options o;
    gt2ref_default_options(&o);
    for (int i = 3; i < argc; i++) {
        const char* v = argv[i] + 2;
        switch (argv[i][0]) {
        case 'A': o.hr_adsr = (unsigned)strtoul(v, 0, 16); break;
        case 'S': o.multiplier = (unsigned)atoi(v); break;
        case 'P': o.player_version = (unsigned)atoi(v); break;
        case 'O': o.optimize_pulse = (unsigned)atoi(v); break;
        case 'R': o.optimize_realtime = (unsigned)atoi(v); break;
        case 'W': o.player_address = (unsigned)strtoul(v, 0, 16) << 8; break;
        case 'Z': o.zeropage = (unsigned)strtoul(v, 0, 16); break;
        case 'G': o.a4_hz = (float)atof(v); break;
        case 'L': o.sid_address = (unsigned)strtoul(v, 0, 16); break;
        case 'N': o.ntsc = 1; break;
        default: return 2;
        }
    }
    unsigned char* out;
    size_t len;
    if (gt2ref_pack(d, (size_t)n, &o, &out, &len) != 0)
        return 1;
    f = fopen(argv[2], "wb");
    if (f == NULL || fwrite(out, 1, len, f) != len)
        return 2;
    fclose(f);
    return 0;
}
