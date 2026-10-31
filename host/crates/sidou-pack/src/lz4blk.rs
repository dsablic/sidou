// SPDX-FileCopyrightText: 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

fn len_bytes(out: &mut Vec<u8>, mut n: usize) {
    while n >= 255 {
        out.push(255);
        n -= 255;
    }
    out.push(n as u8);
}

pub fn compress(src: &[u8]) -> Vec<u8> {
    let n = src.len();
    let mut out = Vec::new();
    let mut table: HashMap<[u8; 4], usize> = HashMap::new();
    let (mut anchor, mut i) = (0usize, 0usize);
    let limit = n.saturating_sub(12);
    while i < limit {
        let key = [src[i], src[i + 1], src[i + 2], src[i + 3]];
        let cand = table.insert(key, i);
        let Some(cand) = cand.filter(|&c| i - c <= 65535) else {
            i += 1;
            continue;
        };
        let mut mlen = 4;
        while i + mlen < n - 5 && src[cand + mlen] == src[i + mlen] {
            mlen += 1;
        }
        let lit = &src[anchor..i];
        out.push(((lit.len().min(15) as u8) << 4) | (mlen - 4).min(15) as u8);
        if lit.len() >= 15 {
            len_bytes(&mut out, lit.len() - 15);
        }
        out.extend_from_slice(lit);
        let off = i - cand;
        out.extend_from_slice(&[off as u8, (off >> 8) as u8]);
        if mlen - 4 >= 15 {
            len_bytes(&mut out, mlen - 4 - 15);
        }
        i += mlen;
        anchor = i;
    }
    let lit = &src[anchor..];
    out.push((lit.len().min(15) as u8) << 4);
    if lit.len() >= 15 {
        len_bytes(&mut out, lit.len() - 15);
    }
    out.extend_from_slice(lit);
    out
}

pub fn decompress(blk: &[u8]) -> Option<Vec<u8>> {
    let (mut out, mut i) = (Vec::new(), 0usize);
    let more = |blk: &[u8], i: &mut usize, mut n: usize| -> Option<usize> {
        loop {
            let b = *blk.get(*i)?;
            *i += 1;
            n += usize::from(b);
            if b != 255 {
                return Some(n);
            }
        }
    };
    while i < blk.len() {
        let tok = blk[i];
        i += 1;
        let mut ll = usize::from(tok >> 4);
        if ll == 15 {
            ll = more(blk, &mut i, ll)?;
        }
        out.extend_from_slice(blk.get(i..i + ll)?);
        i += ll;
        if i >= blk.len() {
            break;
        }
        let off = usize::from(*blk.get(i)?) | usize::from(*blk.get(i + 1)?) << 8;
        i += 2;
        let mut ml = usize::from(tok & 15);
        if ml == 15 {
            ml = more(blk, &mut i, ml)?;
        }
        ml += 4;
        if off == 0 || off > out.len() {
            return None;
        }
        for _ in 0..ml {
            out.push(out[out.len() - off]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for n in [0usize, 1, 5, 12, 13, 100, 4096] {
            let data: Vec<u8> = (0..n).map(|i| (i % 7 * 3 + i / 50) as u8).collect();
            assert_eq!(decompress(&compress(&data)).unwrap(), data, "{n} bytes");
        }
    }

    #[test]
    fn short_input_is_one_literal_run() {
        assert_eq!(compress(b"abc"), b"\x30abc");
        assert_eq!(compress(b""), b"\x00");
    }
}
