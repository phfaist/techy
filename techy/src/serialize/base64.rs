//! Base64 for the JSON rendering of [`SerialValue::Bytes`](super::SerialValue::Bytes):
//! a hand-written encoder and a strict decoder, so that the rendering layer needs no
//! dependency beyond serde. Standard alphabet (`A–Z a–z 0–9 + /`), `=` padding, no
//! line breaks; the decoder accepts exactly the encoder's output (rejects characters
//! outside the alphabet, lengths that are not a multiple of four, padding anywhere but
//! at the end, and non-zero unused bits before the padding), so every byte string has
//! one text form and every accepted text has one byte string.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `bytes` as base64 text.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(char::from(ALPHABET[(n >> 18) as usize & 63]));
        out.push(char::from(ALPHABET[(n >> 12) as usize & 63]));
        out.push(char::from(ALPHABET[(n >> 6) as usize & 63]));
        out.push(char::from(ALPHABET[n as usize & 63]));
    }
    match *chunks.remainder() {
        [a] => {
            let n = u32::from(a) << 16;
            out.push(char::from(ALPHABET[(n >> 18) as usize & 63]));
            out.push(char::from(ALPHABET[(n >> 12) as usize & 63]));
            out.push_str("==");
        }
        [a, b] => {
            let n = (u32::from(a) << 16) | (u32::from(b) << 8);
            out.push(char::from(ALPHABET[(n >> 18) as usize & 63]));
            out.push(char::from(ALPHABET[(n >> 12) as usize & 63]));
            out.push(char::from(ALPHABET[(n >> 6) as usize & 63]));
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Why a text is not valid base64 in the strict form this module produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base64Error {
    /// The text's length is not a multiple of four.
    Length,
    /// A character outside the alphabet (or a `=` before the final padding position),
    /// at this byte offset.
    Character(usize),
    /// The bits left over before the padding are not zero: the text is not the
    /// canonical encoding of any byte string.
    NonCanonical,
}

impl fmt::Display for Base64Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base64Error::Length => write!(f, "base64 text length is not a multiple of four"),
            Base64Error::Character(offset) => {
                write!(f, "invalid base64 character at offset {offset}")
            }
            Base64Error::NonCanonical => {
                write!(f, "base64 text is not the canonical encoding of any byte string")
            }
        }
    }
}

/// Decode strict base64 text (see the module documentation).
pub(crate) fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Base64Error::Length);
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut quads = bytes.chunks_exact(4);
    let last = quads.next_back();
    for (q, quad) in (&mut quads).enumerate() {
        let n = (sextet(quad[0], q * 4)? << 18)
            | (sextet(quad[1], q * 4 + 1)? << 12)
            | (sextet(quad[2], q * 4 + 2)? << 6)
            | sextet(quad[3], q * 4 + 3)?;
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    if let Some(quad) = last {
        let base = bytes.len() - 4;
        let a = sextet(quad[0], base)?;
        let b = sextet(quad[1], base + 1)?;
        match (quad[2], quad[3]) {
            (b'=', b'=') => {
                // One byte: 8 bits used, the low 4 bits of `b` must be zero.
                if b & 0b1111 != 0 {
                    return Err(Base64Error::NonCanonical);
                }
                out.push(((a << 2) | (b >> 4)) as u8);
            }
            (_, b'=') => {
                let c = sextet(quad[2], base + 2)?;
                // Two bytes: 16 bits used, the low 2 bits of `c` must be zero.
                if c & 0b11 != 0 {
                    return Err(Base64Error::NonCanonical);
                }
                let n = (a << 18) | (b << 12) | (c << 6);
                out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8]);
            }
            _ => {
                let c = sextet(quad[2], base + 2)?;
                let d = sextet(quad[3], base + 3)?;
                let n = (a << 18) | (b << 12) | (c << 6) | d;
                out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            }
        }
    }
    Ok(out)
}

/// The six-bit value of one alphabet character; `=` is not part of the alphabet here
/// (padding is handled by the caller at the final positions only).
fn sextet(c: u8, offset: usize) -> Result<u32, Base64Error> {
    let v = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return Err(Base64Error::Character(offset)),
    };
    Ok(u32::from(v))
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, Base64Error};
    use alloc::vec::Vec;

    /// RFC 4648 §10 test vectors, both directions.
    #[test]
    fn rfc4648_vectors() {
        let vectors: [(&str, &str); 7] = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in vectors {
            assert_eq!(encode(plain.as_bytes()), encoded);
            assert_eq!(decode(encoded).unwrap(), plain.as_bytes());
        }
    }

    /// Every byte value appears in the alphabet mapping; the full 256-byte table
    /// round-trips (and uses `+` and `/`).
    #[test]
    fn all_bytes_round_trip() {
        let all: Vec<u8> = (0..=255).collect();
        let text = encode(&all);
        assert!(text.contains('+') && text.contains('/'));
        assert_eq!(decode(&text).unwrap(), all);
        for len in 0..40 {
            let slice = &all[..len];
            assert_eq!(decode(&encode(slice)).unwrap(), slice);
        }
    }

    /// The decoder is strict: it accepts exactly the encoder's output.
    #[test]
    fn strict_rejections() {
        // Missing / short padding.
        assert_eq!(decode("Zg"), Err(Base64Error::Length));
        assert_eq!(decode("Zg="), Err(Base64Error::Length));
        assert_eq!(decode("Zm8"), Err(Base64Error::Length));
        // Characters outside the alphabet: URL-safe alphabet, whitespace, line breaks.
        assert_eq!(decode("Zm9v-g=="), Err(Base64Error::Character(4)));
        assert_eq!(decode("Zm9v_g=="), Err(Base64Error::Character(4)));
        assert_eq!(decode("Zm9v\nYmFy"), Err(Base64Error::Length));
        assert_eq!(decode("Zm9v\nYmF"), Err(Base64Error::Character(4)));
        assert_eq!(decode("Zm9v Yg="), Err(Base64Error::Character(4)));
        // Padding anywhere but the final one or two positions.
        assert_eq!(decode("Zg==Zg=="), Err(Base64Error::Character(2)));
        assert_eq!(decode("Zg=a"), Err(Base64Error::Character(2)));
        assert_eq!(decode("=Zg="), Err(Base64Error::Character(0)));
        assert_eq!(decode("Z==="), Err(Base64Error::Character(1)));
        assert_eq!(decode("===="), Err(Base64Error::Character(0)));
        // Non-canonical: non-zero unused bits before the padding.
        assert_eq!(decode("Zh=="), Err(Base64Error::NonCanonical));
        assert_eq!(decode("Zm9="), Err(Base64Error::NonCanonical));
        // Canonical neighbours of the above are fine.
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
    }
}
