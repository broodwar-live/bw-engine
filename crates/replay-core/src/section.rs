use flate2::read::ZlibDecoder;
use std::io::Read;

use crate::error::{ReplayError, Result};
use crate::format::Format;

/// Reads a little-endian i32 from a byte slice at the given offset.
fn read_i32(data: &[u8], offset: usize) -> Result<i32> {
    let too_short = || ReplayError::Truncated {
        offset,
        len: data.len(),
    };
    let bytes: [u8; 4] = offset
        .checked_add(4)
        .and_then(|end| data.get(offset..end))
        .and_then(|b| b.try_into().ok())
        .ok_or_else(too_short)?;
    Ok(i32::from_le_bytes(bytes))
}

/// Reads a little-endian i32 length/count field; negative values are invalid.
fn read_len(data: &[u8], offset: usize) -> Result<usize> {
    let v = read_i32(data, offset)?;
    usize::try_from(v).map_err(|_| ReplayError::InvalidLength { offset, value: v })
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
/// For modern replays (1.18+), chunks use zlib compression.
/// For legacy replays (pre-1.18), chunks use PKWare DCL Implode compression.
///
/// Returns `(decompressed_section_data, bytes_consumed)`.
pub fn decompress_section(data: &[u8], offset: usize, fmt: Format) -> Result<(Vec<u8>, usize)> {
    let _checksum = read_i32(data, offset)?;
    let chunk_count = read_len(data, offset.saturating_add(4))?;

    let mut cursor = offset.saturating_add(8);
    let mut decompressed = Vec::new();

    for _ in 0..chunk_count {
        let compressed_len = read_len(data, cursor)?;
        cursor += 4;

        let end = cursor.saturating_add(compressed_len);
        let chunk_data = data.get(cursor..end).ok_or(ReplayError::Truncated {
            offset: cursor,
            len: data.len(),
        })?;

        decompress_chunk(chunk_data, fmt, &mut decompressed)?;
        cursor = end;
    }

    Ok((decompressed, cursor - offset))
}

/// Decompress a single chunk, dispatching by format.
fn decompress_chunk(chunk: &[u8], fmt: Format, out: &mut Vec<u8>) -> Result<()> {
    match fmt {
        Format::Legacy => decompress_pkware(chunk, out),
        Format::Modern | Format::Modern121 => decompress_zlib_or_raw(chunk, out),
    }
}

/// Modern format: if the chunk starts with 0x78 (zlib header), decompress;
/// otherwise copy verbatim.
fn decompress_zlib_or_raw(chunk: &[u8], out: &mut Vec<u8>) -> Result<()> {
    if chunk.len() > 4 && chunk[0] == 0x78 {
        let mut decoder = ZlibDecoder::new(chunk);
        let mut buf = Vec::new();
        decoder
            .read_to_end(&mut buf)
            .map_err(|e| ReplayError::Decompression(e.to_string()))?;
        out.extend_from_slice(&buf);
    } else {
        out.extend_from_slice(chunk);
    }
    Ok(())
}

/// Legacy format: decompress using PKWare DCL Implode (via the `explode` crate).
fn decompress_pkware(chunk: &[u8], out: &mut Vec<u8>) -> Result<()> {
    // Very small chunks (≤2 bytes) can't be valid PKWare streams — copy raw.
    if chunk.len() <= 2 {
        out.extend_from_slice(chunk);
        return Ok(());
    }

    match explode::explode(chunk) {
        Ok(decompressed) => {
            out.extend_from_slice(&decompressed);
            Ok(())
        }
        Err(_) => {
            // If decompression fails, the chunk might be uncompressed — copy raw.
            out.extend_from_slice(chunk);
            Ok(())
        }
    }
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

    fn build_section(chunks: &[&[u8]]) -> Vec<u8> {
        let mut section = Vec::new();
        section.extend_from_slice(&0i32.to_le_bytes()); // checksum
        section.extend_from_slice(&(chunks.len() as i32).to_le_bytes());
        for chunk in chunks {
            section.extend_from_slice(&(chunk.len() as i32).to_le_bytes());
            section.extend_from_slice(chunk);
        }
        section
    }

    #[test]
    fn test_decompress_section_single_zlib_chunk() {
        let payload = b"Hello, StarCraft!";
        let compressed = zlib_compress(payload);
        let section = build_section(&[&compressed]);

        let (result, consumed) = decompress_section(&section, 0, Format::Modern).unwrap();
        assert_eq!(result, payload);
        assert_eq!(consumed, section.len());
    }

    #[test]
    fn test_decompress_section_uncompressed_chunk() {
        let payload = b"\x00\x01\x02\x03";
        let section = build_section(&[payload]);

        let (result, _) = decompress_section(&section, 0, Format::Modern).unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn test_decompress_section_multiple_chunks() {
        let part_a = b"Part A";
        let part_b = b"Part B";
        let comp_a = zlib_compress(part_a);
        let comp_b = zlib_compress(part_b);
        let section = build_section(&[&comp_a, &comp_b]);

        let (result, _) = decompress_section(&section, 0, Format::Modern).unwrap();
        assert_eq!(result, b"Part APart B");
    }

    #[test]
    fn test_decompress_section_with_offset() {
        let payload = b"offset test";
        let compressed = zlib_compress(payload);

        let mut data = vec![0xFF; 16];
        let section = build_section(&[&compressed]);
        data.extend_from_slice(&section);

        let (result, _) = decompress_section(&data, 16, Format::Modern).unwrap();
        assert_eq!(result, payload);
    }

    #[test]
    fn test_decompress_section_negative_length_returns_error() {
        let mut section = Vec::new();
        section.extend_from_slice(&0i32.to_le_bytes());
        section.extend_from_slice(&1i32.to_le_bytes());
        section.extend_from_slice(&(-5i32).to_le_bytes());
        let result = decompress_section(&section, 0, Format::Modern);
        assert!(matches!(
            result,
            Err(ReplayError::InvalidLength { value: -5, .. })
        ));
    }

    #[test]
    fn test_decompress_section_offset_past_end_returns_error() {
        assert!(decompress_section(&[0u8; 4], usize::MAX - 2, Format::Modern).is_err());
    }

    #[test]
    fn test_decompress_pkware_known_vector() {
        // Test vector from the explode crate (matches blast.c):
        // Decompresses to "AIAIAIAIAIAIA"
        let compressed: &[u8] = &[0x00, 0x04, 0x82, 0x24, 0x25, 0x8f, 0x80, 0x7f];
        let section = build_section(&[compressed]);

        let (result, _) = decompress_section(&section, 0, Format::Legacy).unwrap();
        assert_eq!(result, b"AIAIAIAIAIAIA");
    }
}
