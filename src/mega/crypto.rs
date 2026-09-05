//! Cryptographic primitives used by MEGA public-file downloads.
//! Algorithms ported exactly from mega.py (odwyersoftware): keys are 8 big-endian
//! u32 words; file contents are AES-128-CTR; attributes are AES-CBC with a zero
//! IV; integrity uses a per-chunk CBC-chained MAC.

use aes::cipher::{Block, BlockCipherEncrypt, BlockModeDecrypt, KeyInit, KeyIvInit, StreamCipher};
use aes::Aes128;
use anyhow::{anyhow, bail, Result};
use base64::Engine;

/// Derived keys for a public file. `nonce` is the CTR IV high half and the MAC
/// IV; `expected_mac` is the two-word MAC the file must match on download.
#[derive(Clone)]
pub struct FileKeys {
    pub aes_key: [u8; 16],
    pub nonce: [u32; 2],
    pub expected_mac: [u32; 2],
}

/// Splits a MEGA public URL into `(file_id, base64url_key)`.
/// Supports the modern `/file/{id}#{key}` and legacy `/#!{id}!{key}` shapes.
pub fn parse_public_url(url: &str) -> Result<(String, String)> {
    if let Some(pos) = url.find("/file/") {
        let rest = &url[pos + 6..];
        let (id, key) = rest
            .split_once('#')
            .ok_or_else(|| anyhow!("MEGA URL missing key fragment"))?;
        return Ok((id.to_string(), key.to_string()));
    }

    if let Some(pos) = url.find("/#") {
        let rest = url[pos + 2..].strip_prefix('!').unwrap_or(&url[pos + 2..]);
        let (id, key) = rest
            .split_once('!')
            .ok_or_else(|| anyhow!("MEGA URL missing key fragment"))?;
        return Ok((id.to_string(), key.to_string()));
    }

    bail!("MEGA URL key missing")
}

/// MEGA base64url decode (accepts `-`/`_`, optional `,` separators, expands
/// padding). Mirrors mega.py's `base64_url_decode`.
pub fn base64_url_decode(input: &str) -> Result<Vec<u8>> {
    let mut cleaned = input.replace('-', "+").replace('_', "/");
    cleaned.retain(|c| c != ',');
    while !cleaned.len().is_multiple_of(4) {
        cleaned.push('=');
    }

    base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|e| anyhow!("invalid MEGA base64: {e}"))
}

/// Derives the file keys from the 32-byte (8-word) key embedded in the URL.
pub fn derive_file_keys(raw_key: &str) -> Result<FileKeys> {
    let bytes = base64_url_decode(raw_key)?;
    if bytes.len() != 32 {
        bail!("unexpected MEGA key length: {} bytes", bytes.len());
    }

    let words: Vec<u32> = bytes
        .as_chunks::<4>().0.iter()
        .map(|chunk| u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    let aes_words = [
        words[0] ^ words[4],
        words[1] ^ words[5],
        words[2] ^ words[6],
        words[3] ^ words[7],
    ];
    let mut aes_key = [0u8; 16];
    for (index, word) in aes_words.iter().enumerate() {
        aes_key[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }

    Ok(FileKeys {
        aes_key,
        nonce: [words[4], words[5]],
        expected_mac: [words[6], words[7]],
    })
}

/// Decrypts the file attributes blob (`at`) and returns the file name.
/// Attributes are AES-CBC with a zero IV, prefixed with `MEGA{` JSON.
pub fn decrypt_attr(encoded: &str, key: &[u8; 16]) -> Result<String> {
    let mut data = base64_url_decode(encoded)?;
    if data.len() % 16 != 0 {
        let padded = data.len().div_ceil(16) * 16;
        data.resize(padded, 0);
    }

    let mut decryptor = cbc::Decryptor::<Aes128>::new(
        &Block::<Aes128>::from(*key),
        &Block::<Aes128>::from([0u8; 16]),
    );
    for chunk in data.as_chunks_mut::<16>().0 {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        let mut generic: Block<Aes128> = block.into();
        decryptor.decrypt_block(&mut generic);
        chunk.copy_from_slice(&generic);
    }

    let text = String::from_utf8_lossy(&data);
    let text = text.trim_end_matches('\0');
    if !text.starts_with("MEGA{\"") {
        bail!("MEGA attribute decryption failed");
    }

    let json_text = &text[4..];
    let end = json_text
        .rfind('}')
        .ok_or_else(|| anyhow!("malformed MEGA attributes"))?;
    let attributes: serde_json::Value = serde_json::from_str(&json_text[..=end])?;
    attributes
        .get("n")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("MEGA attributes do not contain a filename"))
}

/// AES-128-CTR keystream whose 128-bit counter starts at `nonce << 64` and
/// increments across the whole block, exactly like pycryptodome's
/// `Counter.new(128, initial_value=((iv[0] << 32) + iv[1]) << 64)`.
pub struct CtrStream {
    cipher: ctr::Ctr128BE<Aes128>,
}

impl CtrStream {
    pub fn new(keys: &FileKeys) -> Self {
        let nonce = ((keys.nonce[0] as u64) << 32) | keys.nonce[1] as u64;
        let mut iv = [0u8; 16];
        iv[..8].copy_from_slice(&nonce.to_be_bytes());
        Self {
            cipher: ctr::Ctr128BE::<Aes128>::new(
                &Block::<Aes128>::from(keys.aes_key),
                &Block::<Aes128>::from(iv),
            ),
        }
    }

    pub fn apply(&mut self, data: &mut [u8]) {
        self.cipher.apply_keystream(data);
    }
}

/// The MEGA upload chunking scheme: 128 KiB, growing by 128 KiB until 1 MiB,
/// then fixed 1 MiB chunks. Mirrors mega.py's `get_chunks`.
pub fn get_chunks(size: u64) -> Vec<(u64, u64)> {
    let mut chunks = Vec::new();
    let mut position = 0u64;
    let mut chunk_size = 0x20000u64;

    while position + chunk_size < size {
        chunks.push((position, chunk_size));
        position += chunk_size;
        if chunk_size < 0x100000 {
            chunk_size += 0x20000;
        }
    }
    chunks.push((position, size - position));
    chunks
}

/// Incremental file MAC verifier, mirroring mega.py's per-chunk update loop.
///
/// For every chunk a fresh CBC encryptor (IV = nonce||nonce) runs over the
/// plaintext blocks; the resulting last ciphertext block is fed through a
/// second, persistent CBC chain (IV = 0). The final state, folded
/// word-for-word, must equal the key words 6..8.
pub struct ChunkedMac {
    cipher: Aes128,
    nonce_iv: [u8; 16],
    chunk_state: [u8; 16],
    mac_state: [u8; 16],
    chunk_ends: Vec<u64>,
    index: usize,
    consumed: u64,
    expected: [u32; 2],
}

impl ChunkedMac {
    pub fn new(keys: &FileKeys, size: u64) -> Self {
        let mut nonce_iv = [0u8; 16];
        for (index, word) in [keys.nonce[0], keys.nonce[1], keys.nonce[0], keys.nonce[1]]
            .into_iter()
            .enumerate()
        {
            nonce_iv[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }

        let mut chunk_ends = Vec::new();
        let mut end = 0u64;
        for (_, length) in get_chunks(size) {
            end += length;
            chunk_ends.push(end);
        }

        Self {
            cipher: Aes128::new(&Block::<Aes128>::from(keys.aes_key)),
            nonce_iv,
            chunk_state: nonce_iv,
            mac_state: [0u8; 16],
            chunk_ends,
            index: 0,
            consumed: 0,
            expected: keys.expected_mac,
        }
    }

    /// Feeds one 16-byte block of decrypted file data into the MAC.
    pub fn update(&mut self, plaintext_block: &[u8; 16]) {
        self.consumed += 16;

        let mut encrypted = *plaintext_block;
        xor_block(&mut encrypted, &self.chunk_state);
        encrypt_block(&self.cipher, &mut encrypted);
        self.chunk_state = encrypted;

        // Fold into the persistent MAC chain when this block completes a chunk.
        // The final short chunk is zero-padded, so consumed may overshoot.
        if self.index < self.chunk_ends.len() && self.consumed >= self.chunk_ends[self.index] {
            let mut mac = self.mac_state;
            xor_block(&mut mac, &encrypted);
            encrypt_block(&self.cipher, &mut mac);
            self.mac_state = mac;
            self.chunk_state = self.nonce_iv;
            self.index += 1;
        }
    }

    /// Verifies the accumulated MAC against the key material.
    pub fn verify(&self) -> Result<()> {
        if self.chunk_ends.is_empty() {
            return Ok(());
        }
        if self.index != self.chunk_ends.len() {
            bail!("MEGA MAC verification failed: incomplete download data");
        }

        let words: Vec<u32> = self
            .mac_state
            .as_chunks::<4>().0.iter()
            .map(|chunk| u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        let actual = (words[0] ^ words[1], words[2] ^ words[3]);
        let expected = (self.expected[0], self.expected[1]);
        if actual != expected {
            bail!("MEGA MAC verification failed: download is corrupted");
        }
        Ok(())
    }
}

fn xor_block(left: &mut [u8; 16], right: &[u8; 16]) {
    for index in 0..16 {
        left[index] ^= right[index];
    }
}

fn encrypt_block(cipher: &Aes128, block: &mut [u8; 16]) {
    let mut generic = (*block).into();
    cipher.encrypt_block(&mut generic);
    *block = generic.into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_urls() {
        assert_eq!(
            parse_public_url("https://mega.nz/file/abcdefgh#key").unwrap(),
            ("abcdefgh".to_string(), "key".to_string())
        );
        assert_eq!(
            parse_public_url("https://mega.nz/#!abcdefgh!key").unwrap(),
            ("abcdefgh".to_string(), "key".to_string())
        );
    }

    #[test]
    fn chunk_boundaries_follow_mega_scheme() {
        let chunks = get_chunks(0x6000a);
        assert_eq!(chunks[0], (0, 0x20000));
        assert_eq!(chunks[1], (0x20000, 0x40000));
        assert_eq!(chunks.last().unwrap().1, 10);
        assert_eq!(chunks.iter().map(|(_, size)| size).sum::<u64>(), 0x6000a);
    }

    #[test]
    fn value_of_large_chunk_scheme_sums_to_size() {
        for size in [0x20000, 0x100000, 0x7fffF0, 5_000_000_000] {
            let chunks = get_chunks(size);
            let sum = chunks.iter().map(|(_, length)| length).sum::<u64>();
            assert_eq!(sum, size);
        }
    }

    #[test]
    fn decrypts_live_mega_attribute_vector() {
        // redacted.zip from the HackMyVM redirect captured during planning.
        let keys = derive_file_keys("REDACTED").unwrap();
        let name = decrypt_attr(
            "REDACTED",
            &keys.aes_key,
        )
        .unwrap();
        assert_eq!(name, "redacted.zip");
    }
}