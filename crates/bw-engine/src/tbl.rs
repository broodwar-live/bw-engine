//! TBL string table parser for StarCraft: Brood War.
//!
//! TBL files (e.g., `stat_txt.tbl`) contain string tables used for unit names,
//! tech names, upgrade names, and other game text. The format is:
//!
//! ```text
//! [u16 string_count]
//! [u16 offset] * string_count   — byte offset from start of file
//! [null-terminated strings...]
//! ```
//!
//! String offsets point to null-terminated byte strings. Text is typically
//! ASCII or EUC-KR for Korean localization.

use crate::error::{EngineError, Result};

/// A parsed string table.
#[derive(Debug, Clone)]
pub struct StringTable {
    strings: Vec<String>,
}

impl StringTable {
    /// Parse a TBL file from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(EngineError::InvalidTbl("data too short".to_string()));
        }

        let count = u16::from_le_bytes([data[0], data[1]]) as usize;
        let offsets_end = 2 + count * 2;

        if data.len() < offsets_end {
            return Err(EngineError::InvalidTbl(format!(
                "expected at least {offsets_end} bytes for {count} offsets, got {}",
                data.len()
            )));
        }

        let mut strings = Vec::with_capacity(count);

        for i in 0..count {
            let offset = u16::from_le_bytes([data[2 + i * 2], data[2 + i * 2 + 1]]) as usize;

            if offset >= data.len() {
                strings.push(String::new());
                continue;
            }

            let end = data[offset..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| offset + p)
                .unwrap_or(data.len());

            let raw = &data[offset..end];
            strings.push(decode_tbl_string(raw));
        }

        Ok(Self { strings })
    }

    /// Get a string by its 0-based index.
    pub fn get(&self, index: usize) -> Option<&str> {
        self.strings.get(index).map(|s| s.as_str())
    }

    /// Number of strings in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Iterator over all strings.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.strings.iter().map(|s| s.as_str())
    }
}

/// Decode a TBL string, stripping StarCraft color/formatting codes.
///
/// BW uses bytes < 0x20 (except tab/newline) as text color codes.
fn decode_tbl_string(raw: &[u8]) -> String {
    let cleaned: Vec<u8> = raw
        .iter()
        .copied()
        .filter(|&b| b >= 0x20 || b == b'\t' || b == b'\n')
        .collect();

    if cleaned.is_empty() {
        return String::new();
    }

    // Try UTF-8 first, fall back to EUC-KR.
    if let Ok(s) = std::str::from_utf8(&cleaned) {
        s.to_owned()
    } else {
        let (decoded, _, _) = encoding_rs::EUC_KR.decode(&cleaned);
        decoded.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_tbl(strings: &[&[u8]]) -> Vec<u8> {
        let count = strings.len();
        let header_size = 2 + count * 2;
        let mut data = Vec::new();

        // String count.
        data.extend_from_slice(&(count as u16).to_le_bytes());

        // Calculate offsets.
        let mut offset = header_size;
        let mut offsets = Vec::new();
        for s in strings {
            offsets.push(offset as u16);
            offset += s.len() + 1; // +1 for null terminator
        }

        // Write offsets.
        for o in &offsets {
            data.extend_from_slice(&o.to_le_bytes());
        }

        // Write strings.
        for s in strings {
            data.extend_from_slice(s);
            data.push(0); // null terminator
        }

        data
    }

    #[test]
    fn test_parse_tbl() {
        let tbl = build_tbl(&[b"Marine", b"Ghost", b"Vulture"]);
        let table = StringTable::from_bytes(&tbl).unwrap();
        assert_eq!(table.len(), 3);
        assert_eq!(table.get(0), Some("Marine"));
        assert_eq!(table.get(1), Some("Ghost"));
        assert_eq!(table.get(2), Some("Vulture"));
        assert_eq!(table.get(3), None);
    }

    #[test]
    fn test_parse_tbl_empty() {
        let tbl = build_tbl(&[]);
        let table = StringTable::from_bytes(&tbl).unwrap();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_parse_tbl_strips_color_codes() {
        // Byte 0x03 is a color code in BW.
        let tbl = build_tbl(&[b"\x03Marine"]);
        let table = StringTable::from_bytes(&tbl).unwrap();
        assert_eq!(table.get(0), Some("Marine"));
    }

    #[test]
    fn test_parse_tbl_too_short() {
        assert!(StringTable::from_bytes(&[]).is_err());
        assert!(StringTable::from_bytes(&[1]).is_err());
    }

    #[test]
    fn test_tbl_iterator() {
        let tbl = build_tbl(&[b"A", b"B", b"C"]);
        let table = StringTable::from_bytes(&tbl).unwrap();
        let names: Vec<&str> = table.iter().collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }
}
