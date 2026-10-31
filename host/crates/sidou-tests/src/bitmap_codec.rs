// SPDX-FileCopyrightText: 2026 SIDOU contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub seek: usize,
    pub raw: bool,
}

pub fn encode(rows: &[Vec<Vec<u8>>], seeks: &[usize]) -> (Vec<Vec<u16>>, Vec<u8>, Vec<usize>) {
    encode_formats(
        rows,
        &seeks
            .iter()
            .map(|&seek| Format { seek, raw: false })
            .collect::<Vec<_>>(),
    )
}

pub fn encode_formats(
    rows: &[Vec<Vec<u8>>],
    formats: &[Format],
) -> (Vec<Vec<u16>>, Vec<u8>, Vec<usize>) {
    assert_eq!(rows.len(), formats.len());
    let separate: Vec<_> = rows
        .iter()
        .zip(formats)
        .map(|(set, format)| encode_pool(std::slice::from_ref(set), &[format.seek], format.raw))
        .collect();
    if formats.windows(2).all(|s| s[0] == s[1]) {
        let seeks: Vec<_> = formats.iter().map(|f| f.seek).collect();
        let (shared_rows, shared_data) =
            encode_pool(rows, &seeks, formats.first().is_some_and(|f| f.raw));
        if shared_data.len() <= separate.iter().map(|(_, data)| data.len()).sum() {
            return (shared_rows, shared_data, vec![0; rows.len()]);
        }
    }
    let (mut offsets, mut data, mut bases) = (Vec::new(), Vec::new(), Vec::new());
    for (part_rows, part_data) in separate {
        bases.push(data.len());
        offsets.extend(part_rows);
        data.extend(part_data);
    }
    (offsets, data, bases)
}

fn encode_pool(rows: &[Vec<Vec<u8>>], seeks: &[usize], raw: bool) -> (Vec<Vec<u16>>, Vec<u8>) {
    use std::collections::{HashMap, HashSet};

    for (set, &seek) in rows.iter().zip(seeks) {
        assert!(seek.is_power_of_two());
        assert!(
            seek <= 64
                || (seek == 256
                    && set.iter().all(|row| {
                        row.iter().map(|r| usize::from(r >> 4) + 1).sum::<usize>() <= seek
                    }))
        );
    }
    let rows: Vec<Vec<Vec<u8>>> = rows
        .iter()
        .zip(seeks)
        .map(|(set, &seek)| {
            set.iter()
                .map(|row| {
                    let (mut out, mut pixel) = (Vec::new(), 0);
                    for &run in row {
                        let mut left = usize::from(run >> 4) + 1;
                        while left != 0 {
                            let len = left.min(seek - pixel % seek);
                            out.push((((len - 1) << 4) as u8) | (run & 15));
                            pixel += len;
                            left -= len;
                        }
                    }
                    out
                })
                .collect()
        })
        .collect();
    let mut counts = [0usize; 256];
    let mut unique = HashSet::new();
    for (set, &seek) in rows.iter().zip(seeks) {
        for row in set {
            if unique.insert((seek, row)) {
                for &run in row {
                    counts[usize::from(run)] += 1;
                }
            }
        }
    }
    let mut ranked: Vec<u8> = (0..=255).collect();
    ranked.sort_by_key(|&run| (std::cmp::Reverse(counts[usize::from(run)]), run));
    let mut data = if raw {
        vec![0; 15]
    } else {
        ranked[..15].to_vec()
    };
    data.push(
        seeks.first().copied().unwrap_or(64).trailing_zeros() as u8 | if raw { 128 } else { 0 },
    );
    let mut packets = Vec::new();
    let mut seen = HashMap::new();
    let row_ids: Vec<Vec<usize>> = rows
        .iter()
        .zip(seeks)
        .map(|(set, &seek)| {
            set.iter()
                .map(|row| {
                    *seen.entry((row, seek)).or_insert_with(|| {
                        assert!(!row.is_empty(), "a coded row is not empty");
                        let (mut coded, mut packet, mut pixel, mut previous) =
                            (Vec::new(), Vec::new(), 0, 0);
                        for &run in row {
                            if pixel != 0 && pixel % seek == 0 {
                                let value = if raw {
                                    coded.len() / 2
                                } else {
                                    coded.len() - previous
                                };
                                let delta =
                                    u8::try_from(value).expect("a checkpoint within one byte");
                                packet.push(delta);
                                previous = coded.len();
                            }
                            if raw {
                                coded.extend([run & 15, run >> 4]);
                            } else {
                                match data[..15].iter().position(|&v| v == run) {
                                    Some(code) => coded.push(code as u8),
                                    None => coded.extend([15, run & 15, run >> 4]),
                                }
                            }
                            pixel += usize::from(run >> 4) + 1;
                        }
                        assert_eq!(packet.len(), (pixel - 1) / seek);
                        packet.extend(
                            coded
                                .chunks(2)
                                .map(|p| p[0] | (p.get(1).copied().unwrap_or(0) << 4)),
                        );
                        let id = packets.len();
                        packets.push(packet);
                        id
                    })
                })
                .collect()
        })
        .collect();
    let order: Vec<_> = (0..packets.len()).collect();
    let (mut addresses, mut stream) = pack(&packets, &order);
    let mut longest = order.clone();
    longest.sort_by(|&a, &b| {
        packets[b]
            .len()
            .cmp(&packets[a].len())
            .then_with(|| packets[a].cmp(&packets[b]))
    });
    let mut lexical = order.clone();
    lexical.sort_by(|&a, &b| packets[a].cmp(&packets[b]));
    for order in [longest, lexical, order.into_iter().rev().collect()] {
        let (candidate_addresses, candidate) = pack(&packets, &order);
        if candidate.len() < stream.len() {
            addresses = candidate_addresses;
            stream = candidate;
        }
    }
    let offsets = row_ids
        .iter()
        .map(|set| set.iter().map(|&id| addresses[id]).collect())
        .collect();
    data.extend(stream);
    (offsets, data)
}

fn pack(packets: &[Vec<u8>], order: &[usize]) -> (Vec<u16>, Vec<u8>) {
    let mut stream: Vec<u8> = Vec::new();
    let mut offsets = vec![0; packets.len()];
    for &id in order {
        let packet = &packets[id];
        let at = match stream.windows(packet.len()).position(|w| w == packet) {
            Some(at) => at,
            None => {
                let overlap = (0..=stream.len().min(packet.len()))
                    .rev()
                    .find(|&n| stream[stream.len() - n..] == packet[..n])
                    .unwrap();
                let at = stream.len() - overlap;
                stream.extend_from_slice(&packet[overlap..]);
                at
            }
        };
        offsets[id] = u16::try_from(16 + at).expect("bitmap row offset past 64K bytes");
    }
    (offsets, stream)
}

pub fn next(data: &[u8], at: &mut usize) -> u8 {
    if data[15] & 128 != 0 {
        let run = data[*at / 2];
        *at += 2;
        return run;
    }
    let nibble = |i: usize| (data[i / 2] >> ((i & 1) * 4)) & 15;
    let code = nibble(*at);
    *at += 1;
    if code == 15 {
        let run = nibble(*at) | (nibble(*at + 1) << 4);
        *at += 2;
        run
    } else {
        data[usize::from(code)]
    }
}

pub fn nibbles(dictionary: &[u8], run: u8) -> usize {
    if dictionary[15] & 128 != 0 {
        2
    } else if dictionary[..15].contains(&run) {
        1
    } else {
        3
    }
}

pub fn data_ref(base: usize) -> String {
    if base == 0 {
        "RUNS".into()
    } else {
        format!("RUNS + {base}")
    }
}

pub fn seek_row(data: &[u8], row: u16, width: usize, x: usize) -> (usize, usize) {
    let seek = 1usize << (data[15] & 15);
    let row = usize::from(row);
    let checkpoint = x / seek;
    let header = (width - 1) / seek;
    let mut at = 2 * (row + header);
    if data[15] & 128 != 0 {
        if checkpoint != 0 {
            at += 2 * usize::from(data[row + checkpoint - 1]);
        }
    } else {
        for &delta in &data[row..row + checkpoint] {
            at += usize::from(delta);
        }
    }
    (at, checkpoint * seek)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_every_pixel_seeks_to_the_rest_of_the_row(data: &[u8], start: u16, row: &[u8]) {
        let width = row.len();
        for x in 0..width {
            let (mut at, pixel) = seek_row(data, start, width, x);
            let got: Vec<u8> = (pixel..width).map(|_| next(data, &mut at)).collect();
            assert_eq!(&got[x - pixel..], &row[x..]);
        }
    }

    #[test]
    fn raw_packets_keep_run_bytes_and_absolute_checkpoints() {
        let row: Vec<u8> = (0..240).map(|x| (x % 16) as u8).collect();
        let (rows, data, _) = encode_formats(
            &[vec![row.clone()]],
            &[Format {
                seek: 32,
                raw: true,
            }],
        );
        assert_eq!(data[15], 0x85);
        let start = usize::from(rows[0][0]);
        assert_eq!(&data[start..start + 7], &[32, 64, 96, 128, 160, 192, 224]);
        assert_eq!(&data[start + 7..start + 7 + row.len()], &row);
        assert_every_pixel_seeks_to_the_rest_of_the_row(&data, rows[0][0], &row);
    }

    #[test]
    fn short_packets_reuse_a_longer_row_even_when_the_short_row_comes_first() {
        let rows = vec![vec![vec![0, 1, 2, 3], vec![6, 7, 0, 1, 2, 3, 4, 5]]];
        let (_, data, _) = encode(&rows, &[64]);
        assert_eq!(data.len(), 16 + 4);
    }

    #[test]
    fn sharing_never_costs_more_than_independent_pools() {
        let rows = vec![
            vec![(0..300).map(|n| (n % 15) as u8).collect()],
            vec![(0..300).map(|n| (15 + n % 15) as u8).collect()],
        ];
        let alone: usize = rows
            .iter()
            .map(|set| encode(std::slice::from_ref(set), &[64]).1.len())
            .sum();
        let (offsets, data, bases) = encode(&rows, &[64, 64]);
        assert!(data.len() <= alone);
        for ((set, starts), base) in rows.iter().zip(offsets).zip(bases) {
            for (row, start) in set.iter().zip(starts) {
                let want: Vec<u8> = row
                    .iter()
                    .flat_map(|&r| std::iter::repeat_n(r & 15, usize::from(r >> 4) + 1))
                    .collect();
                let (mut at, _) = seek_row(&data[base..], start, want.len(), 0);
                let mut got = Vec::new();
                while got.len() < want.len() {
                    let r = next(&data[base..], &mut at);
                    got.extend(std::iter::repeat_n(r & 15, usize::from(r >> 4) + 1));
                }
                assert_eq!(got, want);
            }
        }
    }

    #[test]
    fn dictionary_codes_and_escapes_decode_at_either_nibble_alignment() {
        let mut data: Vec<u8> = (0..16).map(|n| n * 17).collect();
        data[15] = 6;
        data.extend([0xf2, 0x31, 0x4f, 0x0a]);
        let mut at = 32;
        assert_eq!(next(&data, &mut at), 0x22);
        assert_eq!(at, 33);
        assert_eq!(next(&data, &mut at), 0x31);
        assert_eq!(at, 36);
        assert_eq!(next(&data, &mut at), 0xa4);
        assert_eq!(at, 39);
    }

    #[test]
    fn every_run_round_trips_across_rows_and_sets() {
        let rows = vec![
            (0..=255).map(|r| vec![r]).collect(),
            vec![vec![0x00; 77], vec![0x00, 0xff, 0x12, 0x34, 0x56]],
        ];
        let (offsets, data, bases) = encode(&rows, &[64, 64]);
        assert_eq!(offsets.len(), rows.len());
        for ((set, starts), base) in rows.iter().zip(&offsets).zip(bases) {
            let data = &data[base..];
            assert_eq!(set.len(), starts.len());
            for (row, &start) in set.iter().zip(starts) {
                let width = row.iter().map(|r| usize::from(r >> 4) + 1).sum();
                let (mut at, _) = seek_row(data, start, width, 0);
                let begin = at;
                let got: Vec<u8> = row.iter().map(|_| next(data, &mut at)).collect();
                assert_eq!(got, *row);
                assert_eq!(at - begin, row.iter().map(|&r| nibbles(data, r)).sum());
            }
        }
    }

    #[test]
    fn repeated_runs_take_half_a_byte_and_share_their_packets() {
        let rows = vec![vec![vec![0x00; 33], vec![0x00; 11]], vec![vec![0x00; 33]]];
        let (offsets, data, bases) = encode(&rows, &[64, 64]);
        assert_eq!(data.len(), 16 + 17);
        assert_eq!(offsets[0], [16, 16]);
        assert_eq!(offsets[1], [16]);
        assert_eq!(bases, [0, 0]);
    }

    #[test]
    fn checkpoints_enter_runs_at_the_same_pixel_without_decoding_the_prefix() {
        let rows = vec![vec![(0..24).map(|n| 0xf0 | (n % 16) as u8).collect()]];
        let (offsets, data, bases) = encode(&rows, &[32]);
        let data = &data[bases[0]..];
        for x in (0..384).step_by(32) {
            let (mut at, pixel) = seek_row(data, offsets[0][0], 384, x);
            assert_eq!(pixel, x);
            assert_eq!(next(data, &mut at), 0xf0 | ((x / 16) % 16) as u8);
        }
    }

    #[test]
    fn the_same_pixels_can_have_different_checkpoint_profiles() {
        let row: Vec<u8> = (0..40).map(|x| (x % 16) as u8).collect();
        let sets = vec![vec![row.clone()], vec![row.clone()]];
        let (rows, data, bases) = encode(&sets, &[16, 64]);
        for i in 0..2 {
            assert_every_pixel_seeks_to_the_rest_of_the_row(&data[bases[i]..], rows[i][0], &row);
        }
    }
}
