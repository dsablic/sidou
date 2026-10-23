/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * SIDOU entry, after Felucca's firmware/crt0.S. The SPL maps sidou_app.bin at XIP 0x02000120
 * and jumps to its first byte with r0 = its boot-parameter block; it expects `goto +4; rts`
 * as the first instruction. Interrupts off, both stacks, then C. (Felucca also saves the core
 * state the SPL left for a boot probe, in its debug area below RAM; nothing read those words,
 * and the area is RAMTEXT's now: app.ld.) */
	.section .text.crt0,"ax",@progbits
	.globl _start
_start:
	goto 1f
	rts
1:	cli
	sp = _ustack_top
	ssp = _sstack_top
	call fm1_cstart
	goto .
