//! SHA-256, written here because this crate's dependency list is closed (workspace `Cargo.toml`:
//! «adding one is a decision, not a side effect»). The manifest states a content fingerprint, and a
//! fingerprint that changes with a dependency is worse than eighty lines that never do.
//!
//! Standard FIPS 180-4: a digest of the source's records, so a hop can say *what* it carried.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// A streaming SHA-256: feed it any number of slices, then take the digest. Nothing is copied or
/// buffered beyond one 64-byte block.
pub struct Sha256 {
    h: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Sha256 {
            h: H0,
            block: [0; 64],
            filled: 0,
            length: 0,
        }
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        if self.filled > 0 {
            let take = (64 - self.filled).min(bytes.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&bytes[..take]);
            self.filled += take;
            bytes = &bytes[take..];
            if self.filled < 64 {
                return;
            }
            let block = self.block;
            compress(&mut self.h, &block);
            self.filled = 0;
        }
        let (blocks, rest) = bytes.as_chunks::<64>();
        for block in blocks {
            compress(&mut self.h, block);
        }
        self.block[..rest.len()].copy_from_slice(rest);
        self.filled = rest.len();
    }

    /// The lowercase hex digest of everything fed so far.
    pub fn finalize_hex(mut self) -> String {
        let bit_len = self.length.wrapping_mul(8);
        let mut tail = [0u8; 72];
        tail[0] = 0x80;
        // Pad to 56 mod 64, then the length: one block, or two when the length does not fit.
        let pad = if self.filled < 56 {
            56 - self.filled
        } else {
            120 - self.filled
        };
        tail[pad..pad + 8].copy_from_slice(&bit_len.to_be_bytes());
        let length = self.length;
        self.update(&tail[..pad + 8]);
        self.length = length;
        self.h.iter().map(|word| format!("{word:08x}")).collect()
    }
}

/// An `io::Write` that hashes what is written, so a value can be serialized straight into the
/// digest without building its text first.
impl std::io::Write for Sha256 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize_hex()
}

fn compress(h: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
        w[i] = u32::from_be_bytes(*word);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (state, word) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *state = state.wrapping_add(word);
    }
}

/// The fingerprint of a list of records: SHA-256 over each record's compact JSON text, one per
/// line, in the order the source stored them. It is the digest of the records as decoded, not of
/// the file's bytes: the same records yield the same digest, but whitespace, escapes and number
/// spellings of the file do not count. Each record is serialized straight into the hasher.
pub fn records_sha256<'a>(records: impl IntoIterator<Item = &'a serde_json::Value>) -> String {
    let mut hasher = Sha256::new();
    for record in records {
        // Writing into a hasher cannot fail, and a `Value` always serializes.
        let _ = serde_json::to_writer(&mut hasher, record);
        hasher.update(b"\n");
    }
    hasher.finalize_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIPS 180-4 / NIST vectors — the algorithm, not our memory of it.
    #[test]
    fn matches_the_published_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // 112 bytes: two blocks, one of them with the padding.
        assert_eq!(
            sha256_hex(
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
            ),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
        assert_eq!(
            sha256_hex(&[b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// Lengths around the padding boundaries (55, 56, 63, 64, 119, 120) and every split point of
    /// a streamed input agree with the one-shot digest, and with `sha256sum` (digests computed
    /// with Python's hashlib).
    #[test]
    fn streaming_agrees_at_every_boundary() {
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7 + 3) as u8).collect();
        for len in [0, 1, 55, 56, 57, 63, 64, 65, 119, 120, 121, 128, 255, 300] {
            let whole = sha256_hex(&data[..len]);
            for split in 0..=len {
                let mut hasher = Sha256::new();
                hasher.update(&data[..split]);
                hasher.update(&data[split..len]);
                assert_eq!(hasher.finalize_hex(), whole, "len {len} split {split}");
            }
        }
        assert_eq!(
            sha256_hex(&data[..56]),
            "4324d65f3c103567f5589c710bc08f8523f929a9272e3af36fc968e52abc6c27"
        );
        assert_eq!(
            sha256_hex(&data[..64]),
            "39e3d7b6b5d075d37d053ad89b24b41bef4f3c29760c84447cab3f3be1882241"
        );
        assert_eq!(
            sha256_hex(&data[..120]),
            "7836b787757e95e58b3ca5aec90b1b004e8deba1e50e9675af9cabf1a13a04b5"
        );
    }

    #[test]
    fn a_fingerprint_changes_with_one_byte() {
        let a = records_sha256([&serde_json::json!({"type": "message", "text": "one"})]);
        let b = records_sha256([&serde_json::json!({"type": "message", "text": "two"})]);
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(
            a,
            records_sha256([&serde_json::json!({"type": "message", "text": "one"})])
        );
    }
}
