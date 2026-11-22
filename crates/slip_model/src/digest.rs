//! The 32-byte digest value (`spec/model.md` §3).

use std::fmt;

/// A BLAKE3 digest: 32 bytes. Ordered as a byte string.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    /// The digest's bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses 64 lower-case hexadecimal digits (the only text form).
    pub fn from_hex(text: &str) -> Option<Digest> {
        let text = text.as_bytes();
        if text.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, pair) in text.chunks_exact(2).enumerate() {
            out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
        }
        Some(Digest(out))
    }
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// 64 lower-case hexadecimal digits.
impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::Digest;

    // spec/model.md §3: text form is 64 lower-case hexadecimal digits.
    #[test]
    fn hex_round_trip_and_rejections() {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        let d = Digest(bytes);
        let text = d.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(Digest::from_hex(&text), Some(d));
        // Upper-case digits are not the text form.
        assert!(text.bytes().any(|c| c.is_ascii_lowercase()));
        assert_eq!(Digest::from_hex(&text.to_uppercase()), None);
        assert_eq!(Digest::from_hex(&text[1..]), None);
        assert_eq!(Digest::from_hex(&format!("{text}0")), None);
    }
}
