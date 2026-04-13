//! GRP sprite file parser for StarCraft: Brood War.
//!
//! GRP files contain unit, building, and effect sprites. The format is:
//!
//! ```text
//! Header:
//!   [u16 frame_count]
//!   [u16 width]         — max width of any frame
//!   [u16 height]        — max height of any frame
//!
//! Frame offsets:
//!   [u32 offset] * frame_count — byte offset from start of file
//!
//! Each frame:
//!   [u8  x_offset]   — left padding
//!   [u8  y_offset]   — top padding
//!   [u8  width]      — actual drawn width of this frame
//!   [u8  height]     — actual drawn height of this frame
//!   [u16 row_offset] * height — offsets to each row's RLE data (relative to frame start)
//!
//! Row data (RLE encoded):
//!   Repeat until row_width pixels produced:
//!     byte & 0x80 != 0 → transparent run: (byte & 0x7F) pixels
//!     byte & 0x40 != 0 → color run: (byte & 0x3F) pixels of the next byte's palette index
//!     otherwise        → pixel run: next `byte` bytes are literal palette indices
//! ```

use crate::error::{EngineError, Result};

/// A parsed GRP sprite file.
#[derive(Debug, Clone)]
pub struct Grp {
    /// Max width across all frames.
    pub width: u16,
    /// Max height across all frames.
    pub height: u16,
    /// Individual frames.
    pub frames: Vec<GrpFrame>,
}

/// A single GRP frame with decoded pixel data.
#[derive(Debug, Clone)]
pub struct GrpFrame {
    /// X offset within the GRP bounds.
    pub x_offset: u8,
    /// Y offset within the GRP bounds.
    pub y_offset: u8,
    /// Width of this frame's drawn region.
    pub width: u8,
    /// Height of this frame's drawn region.
    pub height: u8,
    /// Decoded pixel data: palette indices (0 = transparent).
    /// Row-major, `width * height` bytes.
    pub pixels: Vec<u8>,
}

impl Grp {
    /// Parse a GRP file from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 6 {
            return Err(EngineError::InvalidGrp("data too short".to_string()));
        }

        let frame_count = u16::from_le_bytes([data[0], data[1]]) as usize;
        let width = u16::from_le_bytes([data[2], data[3]]);
        let height = u16::from_le_bytes([data[4], data[5]]);

        let offsets_end = 6 + frame_count * 4;
        if data.len() < offsets_end {
            return Err(EngineError::InvalidGrp(format!(
                "expected {offsets_end} bytes for frame offsets, got {}",
                data.len()
            )));
        }

        let mut frames = Vec::with_capacity(frame_count);

        for i in 0..frame_count {
            let frame_offset = u32::from_le_bytes([
                data[6 + i * 4],
                data[6 + i * 4 + 1],
                data[6 + i * 4 + 2],
                data[6 + i * 4 + 3],
            ]) as usize;

            if frame_offset + 4 > data.len() {
                return Err(EngineError::InvalidGrp(format!(
                    "frame {i} offset {frame_offset} out of bounds"
                )));
            }

            let frame = parse_frame(data, frame_offset)?;
            frames.push(frame);
        }

        Ok(Self {
            width,
            height,
            frames,
        })
    }

    /// Number of frames.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

fn parse_frame(data: &[u8], offset: usize) -> Result<GrpFrame> {
    if offset + 8 > data.len() {
        return Err(EngineError::InvalidGrp(
            "frame header truncated".to_string(),
        ));
    }

    let x_offset = data[offset];
    let y_offset = data[offset + 1];
    let width = data[offset + 2];
    let height = data[offset + 3];

    if width == 0 || height == 0 {
        return Ok(GrpFrame {
            x_offset,
            y_offset,
            width,
            height,
            pixels: Vec::new(),
        });
    }

    let row_offsets_start = offset + 4;
    let row_offsets_end = row_offsets_start + height as usize * 2;
    if row_offsets_end > data.len() {
        return Err(EngineError::InvalidGrp(
            "frame row offsets truncated".to_string(),
        ));
    }

    let mut pixels = vec![0u8; width as usize * height as usize];

    for row in 0..height as usize {
        let row_offset = u16::from_le_bytes([
            data[row_offsets_start + row * 2],
            data[row_offsets_start + row * 2 + 1],
        ]) as usize;

        let abs_row_offset = offset + row_offset;
        if abs_row_offset >= data.len() {
            continue; // Skip corrupt rows.
        }

        decode_rle_row(
            &data[abs_row_offset..],
            &mut pixels[row * width as usize..(row + 1) * width as usize],
            width as usize,
        );
    }

    Ok(GrpFrame {
        x_offset,
        y_offset,
        width,
        height,
        pixels,
    })
}

/// Decode one RLE-encoded row of GRP pixel data.
fn decode_rle_row(data: &[u8], row: &mut [u8], row_width: usize) {
    let mut pos = 0; // position in data
    let mut x = 0; // pixel column

    while x < row_width && pos < data.len() {
        let cmd = data[pos];
        pos += 1;

        if cmd & 0x80 != 0 {
            // Transparent run.
            let count = (cmd & 0x7F) as usize;
            x += count; // pixels stay 0 (transparent)
        } else if cmd & 0x40 != 0 {
            // Color run: repeat next byte.
            let count = (cmd & 0x3F) as usize;
            if pos >= data.len() {
                break;
            }
            let color = data[pos];
            pos += 1;
            for _ in 0..count {
                if x < row_width {
                    row[x] = color;
                    x += 1;
                }
            }
        } else {
            // Literal pixel run.
            let count = cmd as usize;
            for _ in 0..count {
                if x < row_width && pos < data.len() {
                    row[x] = data[pos];
                    x += 1;
                    pos += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal GRP with one frame.
    fn build_grp(width: u8, height: u8, rle_rows: &[Vec<u8>]) -> Vec<u8> {
        let frame_count: u16 = 1;
        let max_w = width as u16;
        let max_h = height as u16;

        let mut data = Vec::new();

        // Header.
        data.extend_from_slice(&frame_count.to_le_bytes());
        data.extend_from_slice(&max_w.to_le_bytes());
        data.extend_from_slice(&max_h.to_le_bytes());

        // Frame offset (frame starts right after header + 1 offset).
        let frame_offset = 6 + 4; // header(6) + 1 frame_offset(4)
        data.extend_from_slice(&(frame_offset as u32).to_le_bytes());

        // Frame header.
        data.push(0); // x_offset
        data.push(0); // y_offset
        data.push(width);
        data.push(height);

        // Row offsets (relative to frame start).
        let row_data_start = 4 + height as usize * 2; // frame header + row offsets
        let mut row_offset = row_data_start;
        for rle_row in rle_rows {
            data.extend_from_slice(&(row_offset as u16).to_le_bytes());
            row_offset += rle_row.len();
        }

        // Row RLE data.
        for rle_row in rle_rows {
            data.extend_from_slice(rle_row);
        }

        data
    }

    #[test]
    fn test_parse_grp_transparent_frame() {
        // 4x2 frame, all transparent.
        let row = vec![0x84]; // transparent run of 4 pixels
        let grp_data = build_grp(4, 2, &[row.clone(), row]);
        let grp = Grp::from_bytes(&grp_data).unwrap();

        assert_eq!(grp.frame_count(), 1);
        assert_eq!(grp.width, 4);
        assert_eq!(grp.height, 2);
        assert_eq!(grp.frames[0].pixels, vec![0; 8]);
    }

    #[test]
    fn test_parse_grp_color_run() {
        // 3x1 frame, color run of 3 pixels with palette index 5.
        let row = vec![0x43, 5]; // 0x40 | 3 = color run of 3, color=5
        let grp_data = build_grp(3, 1, &[row]);
        let grp = Grp::from_bytes(&grp_data).unwrap();

        assert_eq!(grp.frames[0].pixels, vec![5, 5, 5]);
    }

    #[test]
    fn test_parse_grp_literal_run() {
        // 3x1 frame, literal run of 3 pixels.
        let row = vec![3, 10, 20, 30]; // literal: 3 bytes follow
        let grp_data = build_grp(3, 1, &[row]);
        let grp = Grp::from_bytes(&grp_data).unwrap();

        assert_eq!(grp.frames[0].pixels, vec![10, 20, 30]);
    }

    #[test]
    fn test_parse_grp_mixed_rle() {
        // 5x1: 2 transparent + color run of 3 with color 7.
        let row = vec![0x82, 0x43, 7]; // trans(2), color_run(3, 7)
        let grp_data = build_grp(5, 1, &[row]);
        let grp = Grp::from_bytes(&grp_data).unwrap();

        assert_eq!(grp.frames[0].pixels, vec![0, 0, 7, 7, 7]);
    }

    #[test]
    fn test_parse_grp_too_short() {
        assert!(Grp::from_bytes(&[0; 4]).is_err());
    }

    #[test]
    fn test_parse_grp_empty_frame() {
        // 0x0 frame — frame offset points to frame header within data.
        let frame_offset: u32 = 10; // 6 (header) + 4 (one frame offset)
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_le_bytes()); // 1 frame
        data.extend_from_slice(&0u16.to_le_bytes()); // width 0
        data.extend_from_slice(&0u16.to_le_bytes()); // height 0
        data.extend_from_slice(&frame_offset.to_le_bytes()); // frame offset
        // Frame header at offset 10: x=0, y=0, w=0, h=0
        data.extend_from_slice(&[0, 0, 0, 0]);
        // Pad to ensure frame header doesn't read past end (need 8 bytes at offset).
        data.extend_from_slice(&[0, 0, 0, 0]);
        let grp = Grp::from_bytes(&data).unwrap();
        assert_eq!(grp.frames[0].pixels.len(), 0);
    }
}
