//! The Zstandard container of a DeepSeek Harness session log (`session.v4.jsonl.zstd`).
//!
//! dsh writes a file of independent frames with the content checksum on: frame 1 is the header
//! line alone, every later frame is one durable batch of lines. A crash leaves the last frame cut
//! off. This module finds the frames without decompressing them (the structural scan dsh's own
//! `scanZstdFrames` does), decompresses each with `ruzstd`, compares the content checksum, and
//! reports a cut-off tail instead of failing the whole file. [`encode_frame`] writes one such frame.

use anyhow::{Context, Result};
use std::io::Read;

const MAGIC: u32 = 0xFD2F_B528;
/// Skippable frames: `0x184D2A50` to `0x184D2A5F`.
const SKIPPABLE: std::ops::RangeInclusive<u32> = 0x184D_2A50..=0x184D_2A5F;

/// One frame of the file, found by the scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub start: usize,
    pub end: usize,
    /// The content checksum the frame states (the low 32 bits of XXH64 of the content).
    pub checksum: Option<u32>,
    pub skippable: bool,
}

/// The frames of a file, and where the last, incomplete one starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scan {
    pub frames: Vec<Frame>,
    pub torn: Option<usize>,
}

/// Whether `bytes` start with a Zstandard (or skippable) frame: a log that is a plain `.jsonl`
/// does not.
pub fn is_zstd(bytes: &[u8]) -> bool {
    u32_at(bytes, 0).is_some_and(|magic| magic == MAGIC || SKIPPABLE.contains(&magic))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Walk the frames of `bytes` without decompressing them. The end of the data inside a frame makes
/// `torn` the offset of that frame; a magic number that is none of Zstandard's, or a reserved bit
/// set, is an error.
pub fn scan(bytes: &[u8]) -> Result<Scan> {
    let mut found = Scan::default();
    let mut at = 0;
    while at < bytes.len() {
        let Some(magic) = u32_at(bytes, at) else {
            found.torn = Some(at);
            break;
        };
        if SKIPPABLE.contains(&magic) {
            let Some(size) = u32_at(bytes, at + 4) else {
                found.torn = Some(at);
                break;
            };
            let end = at + 8 + size as usize;
            if end > bytes.len() {
                found.torn = Some(at);
                break;
            }
            found.frames.push(Frame {
                start: at,
                end,
                checksum: None,
                skippable: true,
            });
            at = end;
            continue;
        }
        if magic != MAGIC {
            anyhow::bail!("byte {at} is not the start of a Zstandard frame (magic {magic:#010x})");
        }
        match frame_end(bytes, at)? {
            Some((end, checksum)) => {
                found.frames.push(Frame {
                    start: at,
                    end,
                    checksum,
                    skippable: false,
                });
                at = end;
            }
            None => {
                found.torn = Some(at);
                break;
            }
        }
    }
    Ok(found)
}

/// The end of the frame that starts at `at`, and its checksum; `None` when the data ends first.
fn frame_end(bytes: &[u8], at: usize) -> Result<Option<(usize, Option<u32>)>> {
    let Some(&descriptor) = bytes.get(at + 4) else {
        return Ok(None);
    };
    if descriptor & 0b0000_1000 != 0 {
        anyhow::bail!("the frame at byte {at} sets a reserved header bit");
    }
    let content_size_flag = descriptor >> 6;
    let single_segment = descriptor & 0b0010_0000 != 0;
    let has_checksum = descriptor & 0b0000_0100 != 0;
    let dictionary_bytes = [0, 1, 2, 4][(descriptor & 0b11) as usize];
    let content_size_bytes = match content_size_flag {
        0 => usize::from(single_segment),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let header = 4 + 1 + usize::from(!single_segment) + dictionary_bytes + content_size_bytes;
    let mut pos = at + header;
    loop {
        let Some(raw) = bytes.get(pos..pos + 3) else {
            return Ok(None);
        };
        let block = u32::from(raw[0]) | u32::from(raw[1]) << 8 | u32::from(raw[2]) << 16;
        let last = block & 1 != 0;
        let size = (block >> 3) as usize;
        let body = match (block >> 1) & 0b11 {
            0 | 2 => size,
            1 => 1,
            _ => anyhow::bail!("the frame at byte {at} has a reserved block type"),
        };
        pos += 3 + body;
        if pos > bytes.len() {
            return Ok(None);
        }
        if last {
            break;
        }
    }
    if has_checksum {
        let Some(checksum) = u32_at(bytes, pos) else {
            return Ok(None);
        };
        return Ok(Some((pos + 4, Some(checksum))));
    }
    Ok(Some((pos, None)))
}

/// What decoding a file gave: each data frame's content, in order, and the offset of a torn tail.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Decoded {
    pub frames: Vec<Vec<u8>>,
    pub torn: Option<usize>,
}

/// Decompress every complete frame of `bytes`, comparing each content checksum. A cut-off last
/// frame is reported in [`Decoded::torn`]; a frame whose content does not match its checksum is an
/// error: the file was damaged after it was written.
pub fn decode(bytes: &[u8]) -> Result<Decoded> {
    let scanned = scan(bytes)?;
    let mut frames = Vec::new();
    for frame in scanned.frames.iter().filter(|f| !f.skippable) {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(&bytes[frame.start..frame.end])
            .map_err(|error| anyhow::anyhow!("{error}"))
            .with_context(|| format!("the frame at byte {} does not decode", frame.start))?;
        let mut content = Vec::new();
        decoder
            .read_to_end(&mut content)
            .with_context(|| format!("the frame at byte {} does not decompress", frame.start))?;
        if let Some(stated) = frame.checksum {
            let computed = xxh64(&content, 0) as u32;
            if computed != stated {
                anyhow::bail!(
                    "the frame at byte {} fails its content checksum (stated {stated:#010x}, \
                     computed {computed:#010x})",
                    frame.start
                );
            }
        }
        frames.push(content);
    }
    Ok(Decoded {
        frames,
        torn: scanned.torn,
    })
}

/// One frame holding `content`, compressed, with the content checksum on, as dsh writes its log
/// frames. `ruzstd`'s encoder leaves the checksum off (it needs the `hash` feature and a hashing
/// crate); the frame header's checksum flag is set here and the 32 bits follow the last block.
pub fn encode_frame(content: &[u8]) -> Vec<u8> {
    let mut frame =
        ruzstd::encoding::compress_to_vec(content, ruzstd::encoding::CompressionLevel::Fastest);
    debug_assert_eq!(u32_at(&frame, 0), Some(MAGIC));
    // Frame_Header_Descriptor, bit 2: Content_Checksum_flag.
    frame[4] |= 0b0000_0100;
    frame.extend((xxh64(content, 0) as u32).to_le_bytes());
    frame
}

/// XXH64, the hash a frame's content checksum is the low 32 bits of.
pub fn xxh64(data: &[u8], seed: u64) -> u64 {
    const P1: u64 = 0x9E37_79B1_85EB_CA87;
    const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
    const P3: u64 = 0x1656_67B1_9E37_79F9;
    const P4: u64 = 0x85EB_CA77_C2B2_AE63;
    const P5: u64 = 0x27D4_EB2F_1656_67C5;
    let round = |acc: u64, input: u64| {
        acc.wrapping_add(input.wrapping_mul(P2))
            .rotate_left(31)
            .wrapping_mul(P1)
    };
    let merge = |acc: u64, value: u64| (acc ^ round(0, value)).wrapping_mul(P1).wrapping_add(P4);
    let word = |at: usize| u64::from_le_bytes(data[at..at + 8].try_into().expect("8 bytes"));
    let mut at = 0;
    let mut hash;
    if data.len() >= 32 {
        let mut v = [
            seed.wrapping_add(P1).wrapping_add(P2),
            seed.wrapping_add(P2),
            seed,
            seed.wrapping_sub(P1),
        ];
        while at + 32 <= data.len() {
            for lane in &mut v {
                *lane = round(*lane, word(at));
                at += 8;
            }
        }
        hash = v[0]
            .rotate_left(1)
            .wrapping_add(v[1].rotate_left(7))
            .wrapping_add(v[2].rotate_left(12))
            .wrapping_add(v[3].rotate_left(18));
        for lane in v {
            hash = merge(hash, lane);
        }
    } else {
        hash = seed.wrapping_add(P5);
    }
    hash = hash.wrapping_add(data.len() as u64);
    while at + 8 <= data.len() {
        hash ^= round(0, word(at));
        hash = hash.rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
        at += 8;
    }
    if at + 4 <= data.len() {
        let value = u32::from_le_bytes(data[at..at + 4].try_into().expect("4 bytes"));
        hash ^= u64::from(value).wrapping_mul(P1);
        hash = hash.rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
        at += 4;
    }
    for &byte in &data[at..] {
        hash ^= u64::from(byte).wrapping_mul(P5);
        hash = hash.rotate_left(11).wrapping_mul(P1);
    }
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(P2);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(P3);
    hash ^ (hash >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of stored (uncompressed) blocks, with the content checksum: a valid Zstandard
    /// frame any decoder reads, written here so the tests need no encoder.
    fn stored_frame(content: &[u8], checksum: bool) -> Vec<u8> {
        let mut out = MAGIC.to_le_bytes().to_vec();
        // Single segment, a four-byte content size, the checksum flag as asked.
        out.push(0b1010_0000 | if checksum { 0b100 } else { 0 });
        out.extend((content.len() as u32).to_le_bytes());
        let mut blocks: Vec<&[u8]> = content.chunks(128 * 1024).collect();
        if blocks.is_empty() {
            blocks.push(&[]);
        }
        for (index, block) in blocks.iter().enumerate() {
            let last = u32::from(index + 1 == blocks.len());
            let header = last | (block.len() as u32) << 3;
            out.extend(&header.to_le_bytes()[..3]);
            out.extend(*block);
        }
        if checksum {
            out.extend((xxh64(content, 0) as u32).to_le_bytes());
        }
        out
    }

    #[test]
    fn xxh64_matches_the_reference_vectors() {
        assert_eq!(xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
        assert_eq!(xxh64(b"a", 0), 0xD24E_C4F1_A98C_6E5B);
        assert_eq!(xxh64(b"abc", 0), 0x44BC_2CF5_AD77_0999);
        assert_eq!(
            xxh64(b"Nobody inspects the spammish repetition", 0),
            0xFBCE_A83C_8A37_8BF1
        );
    }

    #[test]
    fn frames_are_found_without_decompressing_and_a_cut_tail_is_reported() {
        let one = stored_frame(b"{\"type\":\"session\"}\n", true);
        let two = stored_frame(b"{\"a\":1}\n{\"b\":2}\n", true);
        let mut file = [one.clone(), two.clone()].concat();
        let scanned = scan(&file).unwrap();
        assert_eq!(scanned.frames.len(), 2);
        assert_eq!(scanned.frames[1].start, one.len());
        assert_eq!(scanned.torn, None);
        file.extend(&two[..two.len() - 3]);
        let scanned = scan(&file).unwrap();
        assert_eq!(scanned.frames.len(), 2);
        assert_eq!(scanned.torn, Some(one.len() + two.len()));
    }

    #[test]
    fn every_frame_is_decoded_and_its_checksum_compared() {
        let file = [
            stored_frame(b"header\n", true),
            stored_frame(b"one\ntwo\n", true),
            stored_frame(b"three\n", false),
        ]
        .concat();
        let decoded = decode(&file).unwrap();
        assert_eq!(
            decoded.frames,
            [&b"header\n"[..], b"one\ntwo\n", b"three\n"]
        );
        let mut damaged = file.clone();
        let at = scan(&file).unwrap().frames[1].start + 14;
        damaged[at] ^= 0x01;
        let error = format!("{:#}", decode(&damaged).unwrap_err());
        assert!(error.contains("content checksum"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_zstd_is_an_error_and_skippable_frames_are_skipped() {
        assert!(scan(b"{\"type\":\"session\"}\n").is_err());
        let mut file = 0x184D_2A50u32.to_le_bytes().to_vec();
        file.extend(3u32.to_le_bytes());
        file.extend(b"xyz");
        file.extend(stored_frame(b"x\n", true));
        let decoded = decode(&file).unwrap();
        assert_eq!(decoded.frames, [&b"x\n"[..]]);
    }

    #[test]
    fn an_encoded_frame_decodes_with_its_checksum_compared() {
        let content = "{\"type\":\"turn/start\",\"seq\":0}\n".repeat(400);
        let frame = encode_frame(content.as_bytes());
        assert!(frame.len() < content.len() / 4, "it is compressed");
        let scanned = scan(&frame).unwrap();
        assert_eq!(scanned.frames.len(), 1);
        assert_eq!(
            scanned.frames[0].checksum,
            Some(xxh64(content.as_bytes(), 0) as u32)
        );
        let decoded = decode(&frame).unwrap();
        assert_eq!(decoded.frames, [content.as_bytes()]);
        let mut damaged = frame.clone();
        let last = damaged.len() - 1;
        damaged[last] ^= 0x01;
        assert!(format!("{:#}", decode(&damaged).unwrap_err()).contains("checksum"));
        // Frames of different content join into one file, as dsh's log is.
        let file = [encode_frame(b"a\n"), encode_frame(b""), frame].concat();
        assert_eq!(decode(&file).unwrap().frames.len(), 3);
    }

    #[test]
    fn a_header_alone_in_a_torn_file_is_torn_at_zero() {
        let frame = stored_frame(b"header\n", true);
        for cut in [2, 5, 9, 13, frame.len() - 1] {
            let scanned = scan(&frame[..cut]).unwrap();
            assert_eq!(
                (scanned.frames.len(), scanned.torn),
                (0, Some(0)),
                "cut {cut}"
            );
        }
    }
}
