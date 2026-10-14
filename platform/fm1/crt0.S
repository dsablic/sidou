/* SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
 * SPDX-FileCopyrightText: 2026 SIDOU contributors
 * SPDX-License-Identifier: GPL-3.0-only
 * SIDOU entry, after Felucca's firmware/crt0.S. The SPL maps sidou_app.bin at XIP 0x02000120
 * and jumps to its first byte with r0 = its boot-parameter block; it expects `goto +4; rts`
 * as the first instruction. Save the core state the SPL left (for a boot probe, in the
 * debug area below RAM, as Felucca does), interrupts off, both stacks, then C. */
	.section .text.crt0,"ax",@progbits
	.globl _start
_start:
	goto 1f
	rts
1:	r1 = icfg
	r2 = psr
	cli
	r3 = 0x1C07F00		/* debug area, 0x01C06000..0x01C08000 */
	[r3 + 0x20] = r1	/* icfg as handed over */
	[r3 + 0x24] = r2	/* psr */
	[r3 + 0x28] = r0	/* boot-parameter pointer */
	r1 = sp
	[r3 + 0x2C] = r1
	r1 = ssp
	[r3 + 0x30] = r1
	sp = _ustack_top
	ssp = _sstack_top
	call fm1_cstart
	goto .
