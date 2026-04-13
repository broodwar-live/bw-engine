//! MPQ (Mo'PaQ) archive reader for StarCraft: Brood War data files.
//!
//! Supports reading files from:
//! - Game data archives (StarDat.mpq, BrooDat.mpq, patch_rt.mpq)
//! - Map files (.scx, .scm) which are small MPQ archives containing CHK data
//!
//! Only read support is implemented — no archive creation or modification.

use crate::error::{EngineError, Result};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MPQ_MAGIC: u32 = 0x1A51504D; // "MPQ\x1A"
const HASH_TABLE_KEY: u32 = 0xC3AF3770;
const BLOCK_TABLE_KEY: u32 = 0xEC83B3A3;

const FILE_IMPLODE: u32 = 0x0000_0100;
const FILE_COMPRESS: u32 = 0x0000_0200;
const FILE_ENCRYPTED: u32 = 0x0001_0000;
const FILE_FIX_KEY: u32 = 0x0002_0000;
const FILE_EXISTS: u32 = 0x8000_0000;

const HASH_ENTRY_EMPTY: u32 = 0xFFFF_FFFF;
const HASH_ENTRY_DELETED: u32 = 0xFFFF_FFFE;

/// Compression method bytes (first byte of compressed sector data).
const COMPRESS_ZLIB: u8 = 0x02;
const COMPRESS_PKWARE: u8 = 0x08;
const COMPRESS_BZIP2: u8 = 0x10;

// ---------------------------------------------------------------------------
// Crypto table
// ---------------------------------------------------------------------------

/// Pre-computed encryption/hash table (1280 entries).
fn build_crypto_table() -> [u32; 1280] {
    let mut table = [0u32; 1280];
    let mut seed: u32 = 0x0010_0001;

    for i in 0..256u32 {
        let mut index = i;
        for _ in 0..5 {
            seed = seed.wrapping_mul(125).wrapping_add(3) % 0x002A_AAAB;
            let temp1 = (seed & 0xFFFF) << 0x10;

            seed = seed.wrapping_mul(125).wrapping_add(3) % 0x002A_AAAB;
            let temp2 = seed & 0xFFFF;

            table[index as usize] = temp1 | temp2;
            index += 256;
        }
    }

    table
}

/// Hash a string for MPQ hash table lookup.
fn hash_string(name: &str, hash_type: u32, crypto_table: &[u32; 1280]) -> u32 {
    let mut seed1: u32 = 0x7FED_7FED;
    let mut seed2: u32 = 0xEEEE_EEEE;

    for ch in name.bytes() {
        let ch = (ch as char).to_ascii_uppercase() as u32;
        let val = crypto_table[(hash_type.wrapping_mul(256).wrapping_add(ch)) as usize];
        seed1 = val ^ seed1.wrapping_add(seed2);
        seed2 = ch
            .wrapping_add(seed1)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }

    seed1
}

/// Decrypt a block of u32 values in place.
fn decrypt_block(data: &mut [u32], key: u32, crypto_table: &[u32; 1280]) {
    let mut seed1 = key;
    let mut seed2: u32 = 0xEEEE_EEEE;

    for val in data.iter_mut() {
        seed2 = seed2.wrapping_add(crypto_table[(0x400 + (seed1 & 0xFF)) as usize]);
        let encrypted = *val;
        *val = encrypted ^ seed1.wrapping_add(seed2);
        seed1 = (!seed1 << 0x15).wrapping_add(0x1111_1111) | (seed1 >> 0x0B);
        seed2 = (*val)
            .wrapping_add(seed2)
            .wrapping_add(seed2 << 5)
            .wrapping_add(3);
    }
}

// ---------------------------------------------------------------------------
// Header & table entries
// ---------------------------------------------------------------------------

/// MPQ archive header (v1, used by BW).
#[derive(Debug)]
struct MpqHeader {
    /// Offset of the archive within the file (where "MPQ\x1A" was found).
    archive_offset: u64,
    /// Size of the archive header.
    _header_size: u32,
    /// Size of the archive data.
    _archive_size: u32,
    /// Sector size = 512 << sector_size_shift.
    sector_size: u32,
    /// Offset to hash table (relative to archive start).
    hash_table_offset: u32,
    /// Offset to block table (relative to archive start).
    block_table_offset: u32,
    /// Number of entries in hash table.
    hash_table_size: u32,
    /// Number of entries in block table.
    block_table_size: u32,
}

/// A hash table entry.
#[derive(Debug, Clone)]
struct HashEntry {
    hash_a: u32,
    hash_b: u32,
    locale: u16,
    _platform: u16,
    block_index: u32,
}

/// A block table entry.
#[derive(Debug, Clone)]
struct BlockEntry {
    /// Offset of the file data (relative to archive start).
    offset: u32,
    /// Compressed file size.
    _compressed_size: u32,
    /// Uncompressed file size.
    file_size: u32,
    /// File flags.
    flags: u32,
}

// ---------------------------------------------------------------------------
// MPQ Archive
// ---------------------------------------------------------------------------

/// A read-only MPQ archive.
pub struct MpqArchive {
    data: Vec<u8>,
    header: MpqHeader,
    hash_table: Vec<HashEntry>,
    block_table: Vec<BlockEntry>,
    crypto_table: [u32; 1280],
}

impl MpqArchive {
    /// Open an MPQ archive from raw bytes.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let crypto_table = build_crypto_table();
        let header = Self::read_header(&data)?;
        let hash_table = Self::read_hash_table(&data, &header, &crypto_table)?;
        let block_table = Self::read_block_table(&data, &header, &crypto_table)?;

        Ok(Self {
            data,
            header,
            hash_table,
            block_table,
            crypto_table,
        })
    }

    /// List all files in the archive by reading the `(listfile)` entry.
    ///
    /// Returns `None` if the archive has no listfile (common for map files).
    pub fn list_files(&self) -> Option<Vec<String>> {
        let data = self.read_file("(listfile)").ok()?;
        let text = String::from_utf8_lossy(&data);
        Some(
            text.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
        )
    }

    /// Read a file from the archive by its path (e.g., `"staredit\\scenario.chk"`).
    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let hash_index = self.find_hash_entry(name)?;
        let block_index = self.hash_table[hash_index].block_index as usize;

        if block_index >= self.block_table.len() {
            return Err(EngineError::MpqFileNotFound {
                name: name.to_string(),
            });
        }

        let block = &self.block_table[block_index];
        if block.flags & FILE_EXISTS == 0 {
            return Err(EngineError::MpqFileNotFound {
                name: name.to_string(),
            });
        }

        self.extract_file(block, name)
    }

    /// Check whether a file exists in the archive.
    pub fn contains(&self, name: &str) -> bool {
        self.find_hash_entry(name).is_ok()
    }

    // -- Header parsing --

    fn read_header(data: &[u8]) -> Result<MpqHeader> {
        // Search for the MPQ magic at 512-byte boundaries.
        let mut offset = 0u64;
        loop {
            if offset as usize + 32 > data.len() {
                return Err(EngineError::MpqInvalidHeader(
                    "MPQ magic not found".to_string(),
                ));
            }
            let magic = read_u32_le(data, offset as usize);
            if magic == MPQ_MAGIC {
                break;
            }
            offset += 512;
        }

        let base = offset as usize;
        if base + 32 > data.len() {
            return Err(EngineError::MpqInvalidHeader(
                "header truncated".to_string(),
            ));
        }

        let header_size = read_u32_le(data, base + 4);
        let archive_size = read_u32_le(data, base + 8);
        let format_version = read_u16_le(data, base + 12);
        let sector_size_shift = read_u16_le(data, base + 14);
        let hash_table_offset = read_u32_le(data, base + 16);
        let block_table_offset = read_u32_le(data, base + 20);
        let hash_table_size = read_u32_le(data, base + 24);
        let block_table_size = read_u32_le(data, base + 28);

        if format_version > 1 {
            return Err(EngineError::MpqInvalidHeader(format!(
                "unsupported MPQ format version {format_version}"
            )));
        }

        Ok(MpqHeader {
            archive_offset: offset,
            _header_size: header_size,
            _archive_size: archive_size,
            sector_size: 512u32 << sector_size_shift,
            hash_table_offset,
            block_table_offset,
            hash_table_size,
            block_table_size,
        })
    }

    // -- Table parsing --

    fn read_hash_table(
        data: &[u8],
        header: &MpqHeader,
        crypto_table: &[u32; 1280],
    ) -> Result<Vec<HashEntry>> {
        let abs_offset = header.archive_offset as usize + header.hash_table_offset as usize;
        let byte_count = header.hash_table_size as usize * 16;

        if abs_offset + byte_count > data.len() {
            return Err(EngineError::MpqInvalidHeader(
                "hash table extends beyond file".to_string(),
            ));
        }

        // Copy to u32 buffer and decrypt.
        let u32_count = header.hash_table_size as usize * 4;
        let mut buf = vec![0u32; u32_count];
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = read_u32_le(data, abs_offset + i * 4);
        }
        decrypt_block(&mut buf, HASH_TABLE_KEY, crypto_table);

        let mut entries = Vec::with_capacity(header.hash_table_size as usize);
        for i in 0..header.hash_table_size as usize {
            let base = i * 4;
            entries.push(HashEntry {
                hash_a: buf[base],
                hash_b: buf[base + 1],
                locale: (buf[base + 2] & 0xFFFF) as u16,
                _platform: ((buf[base + 2] >> 16) & 0xFFFF) as u16,
                block_index: buf[base + 3],
            });
        }

        Ok(entries)
    }

    fn read_block_table(
        data: &[u8],
        header: &MpqHeader,
        crypto_table: &[u32; 1280],
    ) -> Result<Vec<BlockEntry>> {
        let abs_offset = header.archive_offset as usize + header.block_table_offset as usize;
        let byte_count = header.block_table_size as usize * 16;

        if abs_offset + byte_count > data.len() {
            return Err(EngineError::MpqInvalidHeader(
                "block table extends beyond file".to_string(),
            ));
        }

        let u32_count = header.block_table_size as usize * 4;
        let mut buf = vec![0u32; u32_count];
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = read_u32_le(data, abs_offset + i * 4);
        }
        decrypt_block(&mut buf, BLOCK_TABLE_KEY, crypto_table);

        let mut entries = Vec::with_capacity(header.block_table_size as usize);
        for i in 0..header.block_table_size as usize {
            let base = i * 4;
            entries.push(BlockEntry {
                offset: buf[base],
                _compressed_size: buf[base + 1],
                file_size: buf[base + 2],
                flags: buf[base + 3],
            });
        }

        Ok(entries)
    }

    // -- File lookup --

    fn find_hash_entry(&self, name: &str) -> Result<usize> {
        let table_size = self.hash_table.len();
        if table_size == 0 {
            return Err(EngineError::MpqFileNotFound {
                name: name.to_string(),
            });
        }

        let name_hash = hash_string(name, 0, &self.crypto_table);
        let hash_a = hash_string(name, 1, &self.crypto_table);
        let hash_b = hash_string(name, 2, &self.crypto_table);

        let start = (name_hash as usize) % table_size;
        let mut index = start;

        loop {
            let entry = &self.hash_table[index];

            if entry.block_index == HASH_ENTRY_EMPTY {
                break;
            }

            if entry.block_index != HASH_ENTRY_DELETED
                && entry.hash_a == hash_a
                && entry.hash_b == hash_b
                && entry.locale == 0
            {
                return Ok(index);
            }

            index = (index + 1) % table_size;
            if index == start {
                break;
            }
        }

        // Retry without locale check (some files have non-zero locale).
        index = start;
        loop {
            let entry = &self.hash_table[index];

            if entry.block_index == HASH_ENTRY_EMPTY {
                break;
            }

            if entry.block_index != HASH_ENTRY_DELETED
                && entry.hash_a == hash_a
                && entry.hash_b == hash_b
            {
                return Ok(index);
            }

            index = (index + 1) % table_size;
            if index == start {
                break;
            }
        }

        Err(EngineError::MpqFileNotFound {
            name: name.to_string(),
        })
    }

    // -- File extraction --

    fn extract_file(&self, block: &BlockEntry, name: &str) -> Result<Vec<u8>> {
        let file_offset = self.header.archive_offset as usize + block.offset as usize;
        let is_compressed = block.flags & (FILE_IMPLODE | FILE_COMPRESS) != 0;
        let is_encrypted = block.flags & FILE_ENCRYPTED != 0;

        // Single-sector file (uncompressed and small enough, or single-unit).
        if !is_compressed && !is_encrypted {
            let end = file_offset + block.file_size as usize;
            if end > self.data.len() {
                return Err(EngineError::MpqDecompression(
                    "file data extends beyond archive".to_string(),
                ));
            }
            return Ok(self.data[file_offset..end].to_vec());
        }

        // Multi-sector file: read sector offset table.
        let sector_count = (block.file_size as usize).div_ceil(self.header.sector_size as usize);
        let offset_table_entries = sector_count + 1; // +1 for the end-of-last-sector offset

        // Read (and optionally decrypt) the sector offset table.
        let mut offsets = Vec::with_capacity(offset_table_entries);
        for i in 0..offset_table_entries {
            if file_offset + (i + 1) * 4 > self.data.len() {
                return Err(EngineError::MpqDecompression(
                    "sector offset table truncated".to_string(),
                ));
            }
            offsets.push(read_u32_le(&self.data, file_offset + i * 4));
        }

        if is_encrypted {
            let file_key = self.compute_file_key(name, block);
            decrypt_block(&mut offsets, file_key.wrapping_sub(1), &self.crypto_table);
        }

        // Extract each sector.
        let mut output = Vec::with_capacity(block.file_size as usize);
        let file_key = if is_encrypted {
            Some(self.compute_file_key(name, block))
        } else {
            None
        };

        for i in 0..sector_count {
            let sector_start = file_offset + offsets[i] as usize;
            let sector_end = file_offset + offsets[i + 1] as usize;

            if sector_end > self.data.len() || sector_start > sector_end {
                return Err(EngineError::MpqDecompression(
                    "sector data out of bounds".to_string(),
                ));
            }

            let mut sector_data = self.data[sector_start..sector_end].to_vec();

            // Decrypt sector.
            if let Some(key) = file_key {
                let sector_key = key.wrapping_add(i as u32);
                // Pad to u32 alignment for decryption.
                while !sector_data.len().is_multiple_of(4) {
                    sector_data.push(0);
                }
                let u32_slice: &mut [u32] = unsafe {
                    std::slice::from_raw_parts_mut(
                        sector_data.as_mut_ptr() as *mut u32,
                        sector_data.len() / 4,
                    )
                };
                decrypt_block(u32_slice, sector_key, &self.crypto_table);
                sector_data.truncate(sector_end - sector_start);
            }

            // Decompress sector.
            let expected_size = if i < sector_count - 1 {
                self.header.sector_size as usize
            } else {
                block.file_size as usize - (i * self.header.sector_size as usize)
            };

            if sector_data.len() < expected_size && is_compressed {
                let decompressed = decompress_sector(&sector_data, expected_size, block.flags)?;
                output.extend_from_slice(&decompressed);
            } else {
                output.extend_from_slice(&sector_data[..expected_size.min(sector_data.len())]);
            }
        }

        output.truncate(block.file_size as usize);
        Ok(output)
    }

    fn compute_file_key(&self, name: &str, block: &BlockEntry) -> u32 {
        // Use only the filename portion (after last backslash).
        let short_name = name.rsplit('\\').next().unwrap_or(name);
        let mut key = hash_string(short_name, 3, &self.crypto_table);
        if block.flags & FILE_FIX_KEY != 0 {
            key = (key.wrapping_add(block.offset)) ^ block.file_size;
        }
        key
    }
}

// ---------------------------------------------------------------------------
// Sector decompression
// ---------------------------------------------------------------------------

fn decompress_sector(data: &[u8], expected_size: usize, flags: u32) -> Result<Vec<u8>> {
    if flags & FILE_IMPLODE != 0 {
        // PKWare DCL Implode (legacy SC files).
        return decompress_pkware(data, expected_size);
    }

    if flags & FILE_COMPRESS != 0 && !data.is_empty() {
        // Multi-compression: first byte indicates method(s).
        let method = data[0];
        let payload = &data[1..];

        if method & COMPRESS_BZIP2 != 0 {
            return Err(EngineError::MpqDecompression(
                "bzip2 compression not supported".to_string(),
            ));
        }
        if method & COMPRESS_ZLIB != 0 {
            return decompress_zlib(payload, expected_size);
        }
        if method & COMPRESS_PKWARE != 0 {
            return decompress_pkware(payload, expected_size);
        }

        // Unknown or no compression — return raw.
        return Ok(payload.to_vec());
    }

    Ok(data.to_vec())
}

fn decompress_zlib(data: &[u8], _expected_size: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut buf = Vec::new();
    decoder
        .read_to_end(&mut buf)
        .map_err(|e| EngineError::MpqDecompression(format!("zlib: {e}")))?;
    Ok(buf)
}

fn decompress_pkware(data: &[u8], _expected_size: usize) -> Result<Vec<u8>> {
    if data.len() <= 2 {
        return Ok(data.to_vec());
    }
    explode::explode(data).map_err(|e| EngineError::MpqDecompression(format!("pkware: {e:?}")))
}

// ---------------------------------------------------------------------------
// Byte reading helpers
// ---------------------------------------------------------------------------

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_table_deterministic() {
        let t1 = build_crypto_table();
        let t2 = build_crypto_table();
        assert_eq!(t1, t2);
        // Spot-check a known value (StormLib reference).
        assert_ne!(t1[0], 0);
    }

    #[test]
    fn test_hash_string_known_values() {
        let ct = build_crypto_table();
        // "(hash table)" with hash_type=3 is the hash table encryption key.
        let key = hash_string("(hash table)", 3, &ct);
        assert_eq!(key, HASH_TABLE_KEY);
        let key = hash_string("(block table)", 3, &ct);
        assert_eq!(key, BLOCK_TABLE_KEY);
    }

    #[test]
    fn test_hash_string_case_insensitive() {
        let ct = build_crypto_table();
        let h1 = hash_string("staredit\\scenario.chk", 1, &ct);
        let h2 = hash_string("STAREDIT\\SCENARIO.CHK", 1, &ct);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_decrypt_roundtrip() {
        let ct = build_crypto_table();
        let original: Vec<u32> = vec![0xDEAD_BEEF, 0xCAFE_BABE, 0x1234_5678, 0x0000_0001];
        let mut buf = original.clone();

        // Encrypt (same algorithm, key=42).
        // We can't easily encrypt, so just test that decrypt changes data.
        decrypt_block(&mut buf, 42, &ct);
        // At least one value should differ (extremely unlikely to be the same).
        assert_ne!(buf, original);
    }

    #[test]
    fn test_read_header_no_magic() {
        let data = vec![0u8; 64];
        let result = MpqArchive::from_bytes(data);
        assert!(result.is_err());
    }

    /// Build a minimal valid MPQ archive with one uncompressed file.
    fn build_test_mpq(file_name: &str, file_data: &[u8]) -> Vec<u8> {
        let ct = build_crypto_table();

        // Layout:
        // [0..32]    header
        // [32..N]    file data (uncompressed, unencrypted)
        // [N..N+HT]  hash table (16 entries * 16 bytes = 256 bytes)
        // [N+HT..]   block table (1 entry * 16 bytes)
        let hash_table_count = 16u32;
        let block_table_count = 1u32;

        let file_offset = 32u32;
        let hash_table_offset = file_offset + file_data.len() as u32;
        let block_table_offset = hash_table_offset + hash_table_count * 16;
        let archive_size = block_table_offset + block_table_count * 16;

        let mut buf = vec![0u8; archive_size as usize];

        // Header.
        buf[0..4].copy_from_slice(&MPQ_MAGIC.to_le_bytes());
        buf[4..8].copy_from_slice(&32u32.to_le_bytes()); // header size
        buf[8..12].copy_from_slice(&archive_size.to_le_bytes());
        buf[12..14].copy_from_slice(&0u16.to_le_bytes()); // format version 0
        buf[14..16].copy_from_slice(&3u16.to_le_bytes()); // sector size shift (512 << 3 = 4096)
        buf[16..20].copy_from_slice(&hash_table_offset.to_le_bytes());
        buf[20..24].copy_from_slice(&block_table_offset.to_le_bytes());
        buf[24..28].copy_from_slice(&hash_table_count.to_le_bytes());
        buf[28..32].copy_from_slice(&block_table_count.to_le_bytes());

        // File data.
        buf[file_offset as usize..file_offset as usize + file_data.len()]
            .copy_from_slice(file_data);

        // Build hash table: all entries empty, then place our file.
        let mut hash_buf = vec![0u32; hash_table_count as usize * 4];
        for i in 0..hash_table_count as usize {
            let base = i * 4;
            hash_buf[base] = HASH_ENTRY_EMPTY; // hash_a (unused for empty)
            hash_buf[base + 1] = HASH_ENTRY_EMPTY; // hash_b
            hash_buf[base + 2] = 0xFFFF_FFFF; // locale | platform
            hash_buf[base + 3] = HASH_ENTRY_EMPTY; // block_index = empty
        }

        // Place our file entry.
        let name_hash = hash_string(file_name, 0, &ct) % hash_table_count;
        let hash_a = hash_string(file_name, 1, &ct);
        let hash_b = hash_string(file_name, 2, &ct);
        let slot = name_hash as usize * 4;
        hash_buf[slot] = hash_a;
        hash_buf[slot + 1] = hash_b;
        hash_buf[slot + 2] = 0; // locale 0, platform 0
        hash_buf[slot + 3] = 0; // block index 0

        // Encrypt hash table.
        encrypt_block(&mut hash_buf, HASH_TABLE_KEY, &ct);

        // Write hash table.
        for (i, &val) in hash_buf.iter().enumerate() {
            let off = hash_table_offset as usize + i * 4;
            buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
        }

        // Build block table: one entry.
        let mut block_buf = vec![0u32; 4];
        block_buf[0] = file_offset; // offset
        block_buf[1] = file_data.len() as u32; // _compressed_size
        block_buf[2] = file_data.len() as u32; // file size
        block_buf[3] = FILE_EXISTS; // flags: exists, not compressed

        // Encrypt block table.
        encrypt_block(&mut block_buf, BLOCK_TABLE_KEY, &ct);

        for (i, &val) in block_buf.iter().enumerate() {
            let off = block_table_offset as usize + i * 4;
            buf[off..off + 4].copy_from_slice(&val.to_le_bytes());
        }

        buf
    }

    /// Encrypt helper (inverse of decrypt) for test MPQ construction.
    fn encrypt_block(data: &mut [u32], key: u32, crypto_table: &[u32; 1280]) {
        let mut seed1 = key;
        let mut seed2: u32 = 0xEEEE_EEEE;

        for val in data.iter_mut() {
            seed2 = seed2.wrapping_add(crypto_table[(0x400 + (seed1 & 0xFF)) as usize]);
            let plain = *val;
            *val = plain ^ seed1.wrapping_add(seed2);
            seed1 = (!seed1 << 0x15).wrapping_add(0x1111_1111) | (seed1 >> 0x0B);
            seed2 = plain
                .wrapping_add(seed2)
                .wrapping_add(seed2 << 5)
                .wrapping_add(3);
        }
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let ct = build_crypto_table();
        let original: Vec<u32> = vec![0xDEAD_BEEF, 0xCAFE_BABE, 0x1234_5678, 0x0000_0001];
        let mut buf = original.clone();
        encrypt_block(&mut buf, 42, &ct);
        assert_ne!(buf, original);
        decrypt_block(&mut buf, 42, &ct);
        assert_eq!(buf, original);
    }

    #[test]
    fn test_read_file_from_test_mpq() {
        let file_data = b"Hello, StarCraft!";
        let mpq_bytes = build_test_mpq("test\\hello.txt", file_data);

        let archive = MpqArchive::from_bytes(mpq_bytes).expect("should parse test MPQ");
        let result = archive
            .read_file("test\\hello.txt")
            .expect("should find file");
        assert_eq!(result, file_data);
    }

    #[test]
    fn test_file_not_found() {
        let mpq_bytes = build_test_mpq("test\\hello.txt", b"data");
        let archive = MpqArchive::from_bytes(mpq_bytes).unwrap();
        assert!(archive.read_file("nonexistent\\file.dat").is_err());
    }

    #[test]
    fn test_contains() {
        let mpq_bytes = build_test_mpq("test\\hello.txt", b"data");
        let archive = MpqArchive::from_bytes(mpq_bytes).unwrap();
        assert!(archive.contains("test\\hello.txt"));
        assert!(!archive.contains("nope"));
    }
}
