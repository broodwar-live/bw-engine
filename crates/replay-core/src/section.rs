use flate2::read::ZlibDecoder;
use std::io::Read;

use crate::error::{ReplayError, Result};

/// Reads a little-endian i32 from a byte slice at the given offset.
fn read_i32(data: &[u8], offset: usize) -> Result<i32> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or(ReplayError::TooShort {
            expected: offset + 4,
            actual: data.len(),
        })?
        .try_into()
        .unwrap();
    Ok(i32::from_le_bytes(bytes))
}

/// Decompress a single section from the replay data stream.
///
/// Each section starts with an 8-byte envelope:
///   - 4 bytes: i32 checksum (ignored)
///   - 4 bytes: i32 chunk_count
///
/// Followed by `chunk_count` chunks, each:
///   - 4 bytes: i32 compressed_length
///   - `compressed_length` bytes: compressed data
///
/// Returns `(decompressed_section_data, bytes_consumed)`.
pub fn decompress_section(data: &[u8], offset: usize) -> Result<(Vec<u8>, usize)> {
    let _checksum = read_i32(data, offset)?;
    let chunk_count = read_i32(data, offset + 4)? as usize;

    let mut cursor = offset + 8;
    let mut decompressed = Vec::new();

    for _ in 0..chunk_count {
        let compressed_len = read_i32(data, cursor)? as usize;
        cursor += 4;

        let chunk_data =
            data.get(cursor..cursor + compressed_len)
                .ok_or(ReplayError::TooShort {
                    expected: cursor + compressed_len,
                    actual: data.len(),
                })?;

        // If data starts with 0x78 (zlib header), decompress; otherwise copy verbatim.
        if compressed_len > 4 && chunk_data[0] == 0x78 {
            let mut decoder = ZlibDecoder::new(chunk_data);
            let mut buf = Vec::new();
            decoder
                .read_to_end(&mut buf)
                .map_err(|e| ReplayError::Decompression(e.to_string()))?;
            decompressed.extend_from_slice(&buf);
        } else {
            decompressed.extend_from_slice(chunk_data);
        }

        cursor += compressed_len;
    }

    Ok((decompressed, cursor - offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn zlib_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn test_decompress_section_single_zlib_chunk() {
        let payload = b"Hello, StarCraft!";
        let compressed = zlib_compress(payload);

        // Build section: checksum(4) + chunk_count(4) + chunk_len(4) + compressed_data
        let mut section = Vec::new();
        section.extend_from_slice(&0i32.to_le_bytes()); // checksum
        section.extend_from_slice(&1i32.to_le_bytes()); // 1 chunk
        section.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        section.extend_from_slice(&compressed);

        let (result, consumed) = decompress_section(&section, 0).unwrap();
        assert_eq!(result, payload);
        assert_eq!(consumed, section.len());
    }

    #[test]
    fn test_decompress_section_uncompressed_chunk() {
        let payload = b"\x00\x01\x02\x03"; // doesn't start with 0x78
        let mut section = Vec::new();
        section.extend_from_slice(&0i32.to_le_bytes());
        section.extend_from_slice(&1i32.to_le_bytes());
        section.extend_from_slice(&(payload.len() as i32).to_le_bytes());
        section.extend_from_slice(payload);

        let (result, _) = decompress_section(&section, 0).unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn test_decompress_section_multiple_chunks() {
        let part_a = b"Part A";
        let part_b = b"Part B";
        let comp_a = zlib_compress(part_a);
        let comp_b = zlib_compress(part_b);

        let mut section = Vec::new();
        section.extend_from_slice(&0i32.to_le_bytes());
        section.extend_from_slice(&2i32.to_le_bytes());
        section.extend_from_slice(&(comp_a.len() as i32).to_le_bytes());
        section.extend_from_slice(&comp_a);
        section.extend_from_slice(&(comp_b.len() as i32).to_le_bytes());
        section.extend_from_slice(&comp_b);

        let (result, _) = decompress_section(&section, 0).unwrap();
        assert_eq!(result, b"Part APart B");
    }

    #[test]
    fn test_decompress_section_with_offset() {
        let payload = b"offset test";
        let compressed = zlib_compress(payload);

        let mut data = vec![0xFF; 16]; // 16 bytes of garbage before section
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&1i32.to_le_bytes());
        data.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        data.extend_from_slice(&compressed);

        let (result, _) = decompress_section(&data, 16).unwrap();
        assert_eq!(result, payload);
    }
}
