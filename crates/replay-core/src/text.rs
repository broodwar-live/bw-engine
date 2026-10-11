//! Text decoding shared by the header, player-name, and chat parsers.

/// Decode a null-terminated BW byte string into UTF-8.
///
/// Remastered (1.18+) writes UTF-8, while older Korean clients write EUC-KR
/// (CP949). UTF-8 is tried first; if the bytes are not valid UTF-8 they are
/// decoded as EUC-KR. StarCraft text colour/formatting control characters
/// (bytes `< 0x20` other than tab and newline) are stripped.
#[must_use]
pub fn decode_bw_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let raw: Vec<u8> = bytes[..end]
        .iter()
        .copied()
        .filter(|&b| b >= 0x20 || b == b'\t' || b == b'\n')
        .collect();

    if raw.is_empty() {
        return String::new();
    }

    match String::from_utf8(raw) {
        Ok(s) => s,
        Err(e) => {
            let (decoded, _, _) = encoding_rs::EUC_KR.decode(e.as_bytes());
            decoded.into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_bw_string_utf8_returns_text() {
        assert_eq!(decode_bw_string("이영호\0junk".as_bytes()), "이영호");
    }

    #[test]
    fn test_decode_bw_string_euc_kr_falls_back() {
        // "이영호" encoded as EUC-KR.
        let euc_kr = [0xC0, 0xCC, 0xBF, 0xB5, 0xC8, 0xA3, 0x00, 0xFF];
        assert_eq!(decode_bw_string(&euc_kr), "이영호");
    }

    #[test]
    fn test_decode_bw_string_strips_control_chars() {
        assert_eq!(decode_bw_string(b"\x03gg\x04 wp"), "gg wp");
    }

    #[test]
    fn test_decode_bw_string_empty_returns_empty() {
        assert_eq!(decode_bw_string(b"\0abc"), "");
        assert_eq!(decode_bw_string(b""), "");
    }
}
