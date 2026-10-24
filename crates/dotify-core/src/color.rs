use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An 8-bit RGBA color with straight (non-premultiplied) alpha.
///
/// Serializes as a `#rrggbbaa` hex string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);
    pub const BLACK: Rgba = Rgba::new(0, 0, 0, 255);
    pub const WHITE: Rgba = Rgba::new(255, 255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub const fn from_array(c: [u8; 4]) -> Self {
        Self::new(c[0], c[1], c[2], c[3])
    }

    /// Two colors are "the same pixel" for fill/replace purposes when they are
    /// identical, or when both are fully transparent (RGB of an invisible
    /// pixel is irrelevant).
    pub fn matches(self, other: Rgba) -> bool {
        self == other || (self.a == 0 && other.a == 0)
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }
}

impl fmt::Debug for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid color {0:?}: expected #rgb, #rrggbb or #rrggbbaa")]
pub struct ParseColorError(pub String);

impl FromStr for Rgba {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_string());
        let hex = s.strip_prefix('#').ok_or_else(err)?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        match hex.len() {
            3 => {
                let n = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).map(|v| v * 17).map_err(|_| err());
                Ok(Rgba::opaque(n(0)?, n(1)?, n(2)?))
            }
            6 => Ok(Rgba::opaque(byte(0)?, byte(2)?, byte(4)?)),
            8 => Ok(Rgba::new(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => Err(err()),
        }
    }
}

impl Serialize for Rgba {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!("#fff".parse::<Rgba>().unwrap(), Rgba::WHITE);
        assert_eq!("#102030".parse::<Rgba>().unwrap(), Rgba::opaque(0x10, 0x20, 0x30));
        assert_eq!("#10203040".parse::<Rgba>().unwrap(), Rgba::new(0x10, 0x20, 0x30, 0x40));
        assert!("102030".parse::<Rgba>().is_err());
        assert!("#12345".parse::<Rgba>().is_err());
        assert!("#gg0000".parse::<Rgba>().is_err());
    }

    #[test]
    fn serde_round_trip() {
        let c = Rgba::new(1, 2, 3, 4);
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"#01020304\"");
        assert_eq!(serde_json::from_str::<Rgba>(&json).unwrap(), c);
    }

    #[test]
    fn transparent_pixels_match_regardless_of_rgb() {
        assert!(Rgba::new(10, 20, 30, 0).matches(Rgba::TRANSPARENT));
        assert!(!Rgba::new(10, 20, 30, 1).matches(Rgba::new(10, 20, 30, 2)));
    }
}
