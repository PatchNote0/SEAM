//! Evidence records: the source lines that prove each claim about a Seam.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::span::Span;

/// BLAKE3 of a whole file, taken when the evidence was recorded.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn of(bytes: &[u8]) -> ContentHash {
        ContentHash(*blake3::hash(bytes).as_bytes())
    }

    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn from_hex(text: &str) -> Option<ContentHash> {
        if text.len() != 64 || !text.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(ContentHash(bytes))
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", &self.to_hex()[..12])
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        ContentHash::from_hex(&text)
            .ok_or_else(|| serde::de::Error::custom("expected 64 hex characters"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    ConsumerCall,
    ProducerRoute,
    ContractOperation,
    ConsumerType,
    ProducerModel,
    ContractSchema,
    /// A deterministic step that connected two facts: an import followed, a
    /// constant propagated, a router prefix composed, a wrapper resolved.
    Resolution,
    /// The code that made an assumption necessary.
    Assumption,
    Test,
}

/// Which adapter produced a fact, and how.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Method {
    /// Adapter name and version, for example `typescript@0.1`.
    pub adapter: String,
    /// The syntactic technique, for example `call-expression:fetch`.
    pub technique: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub role: Role,
    pub span: Span,
    pub excerpt: String,
    pub file_hash: ContentHash,
    pub method: Method,
    /// One sentence saying what this record proves.
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_round_trip_through_hex() {
        let hash = ContentHash::of(b"fetch('/accounts')");
        assert_eq!(ContentHash::from_hex(&hash.to_hex()), Some(hash));
        assert_eq!(ContentHash::from_hex("zz"), None);
        let json = serde_json::to_string(&hash).unwrap_or_default();
        assert_eq!(json.len(), 66);
    }
}
