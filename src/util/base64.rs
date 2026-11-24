//! Base64 decoder for IsoMapPack5 and other binary sections in RA2 .map files.
//!
//! Standard alphabet (A-Za-z0-9+/), `=` padding. Whitespace and newlines are
//! skipped since map files split base64 data across numbered INI keys.
//!
//! ## Dependency rules
//! - Part of util/ — no dependencies on game modules.

// Outside data reaches this module: it refuses bad input, never panics.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

/// Decode a base64 string to raw bytes.
///
/// Skips any whitespace/newline characters (map files store base64 data across
/// multiple numbered INI lines). Returns an error if invalid characters are found.
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    // Build a flat buffer of 6-bit values, skipping whitespace.
    let mut sextet_buf: Vec<u8> = Vec::with_capacity(input.len());
    let mut padding: usize = 0;

    for ch in input.chars() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        if ch == '=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            return Err("Invalid base64: data after padding".into());
        }
        let val: u8 = decode_char(ch)?;
        sextet_buf.push(val);
    }

    // Community map editors wrap IsoMapPack5 at a fixed column and can leave
    // the final group one sextet short, or append stray `=` padding. Keep the
    // whole groups and drop that tail instead of refusing the map: outside
    // data must not stop the game, and well-formed data still decodes exactly
    // as before (the decode loop below only emits bytes for 2/3/4-sextet
    // groups, so a lone trailing sextet contributes nothing).
    if sextet_buf.len() % 4 == 1 {
        sextet_buf.pop();
    }

    // Each group of 4 sextets → 3 bytes. Final group may produce 1 or 2 bytes.
    let full_groups: usize = sextet_buf.len() / 4;
    let mut output: Vec<u8> = Vec::with_capacity(full_groups * 3 + 2);

    for group in sextet_buf.chunks(4) {
        if group.len() == 4 {
            let combined: u32 = (group[0] as u32) << 18
                | (group[1] as u32) << 12
                | (group[2] as u32) << 6
                | (group[3] as u32);
            output.push((combined >> 16) as u8);
            output.push((combined >> 8) as u8);
            output.push(combined as u8);
        } else if group.len() == 3 {
            // 3 sextets → 2 bytes (1 padding char).
            let combined: u32 =
                (group[0] as u32) << 18 | (group[1] as u32) << 12 | (group[2] as u32) << 6;
            output.push((combined >> 16) as u8);
            output.push((combined >> 8) as u8);
        } else if group.len() == 2 {
            // 2 sextets → 1 byte (2 padding chars).
            let combined: u32 = (group[0] as u32) << 18 | (group[1] as u32) << 12;
            output.push((combined >> 16) as u8);
        }
    }

    Ok(output)
}

/// Decode a single base64 character to its 6-bit value.
fn decode_char(ch: char) -> Result<u8, String> {
    match ch {
        'A'..='Z' => Ok(ch as u8 - b'A'),
        'a'..='z' => Ok(ch as u8 - b'a' + 26),
        '0'..='9' => Ok(ch as u8 - b'0' + 52),
        '+' => Ok(62),
        '/' => Ok(63),
        _ => Err(format!("Invalid base64 character: {:?}", ch)),
    }
}

/// Encode bytes as base64 with the standard alphabet and `=` padding, the
/// form [`base64_decode`] reads.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let combined = u32::from(group[0]) << 16
            | u32::from(*group.get(1).unwrap_or(&0)) << 8
            | u32::from(*group.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= group.len() {
                out.push(ALPHABET[(combined >> (18 - 6 * index) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(all(test, tests_util))]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        // "Hello" = SGVsbG8=
        assert_eq!(base64_decode("SGVsbG8=").unwrap(), b"Hello".to_vec());
    }

    #[test]
    fn test_whitespace_handling() {
        // Simulates how .map files split base64 across lines.
        let input = "SGVs\n  bG8=\n";
        assert_eq!(base64_decode(input).unwrap(), b"Hello".to_vec());
    }

    #[test]
    fn test_invalid_char() {
        assert!(base64_decode("SGVs!G8=").is_err());
    }

    #[test]
    fn test_binary_roundtrip() {
        // Verify decoding of known binary data.
        // [0x00, 0xFF, 0x80] = AP+A
        let result: Vec<u8> = base64_decode("AP+A").unwrap();
        assert_eq!(result, vec![0x00, 0xFF, 0x80]);
    }

    /// Lenient tail handling must leave well-formed data byte-for-byte alone.
    #[test]
    fn test_padded_roundtrip_is_unchanged() {
        for bytes in [
            b"Hello".to_vec(),
            b"Hello!".to_vec(),
            vec![0x00, 0xFF, 0x80],
            Vec::new(),
        ] {
            let encoded = base64_encode(&bytes);
            assert_eq!(base64_decode(&encoded).unwrap(), bytes, "{encoded}");
        }
    }

    /// Community map editors wrap IsoMapPack5 at a fixed column and can leave
    /// one sextet past the last full group (length % 4 == 1). Drop that tail
    /// instead of refusing the map.
    #[test]
    fn test_short_final_group_is_dropped() {
        assert_eq!(base64_decode("QUJDQ").unwrap(), b"ABC".to_vec());
    }

    #[test]
    fn test_stray_padding_is_ignored() {
        assert_eq!(base64_decode("QUJD====").unwrap(), b"ABC".to_vec());
    }
}
