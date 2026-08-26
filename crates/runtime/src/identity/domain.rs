use std::{fmt, str::FromStr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    MalformedStoredSecret,
    InvalidPublicIdentity,
    StoreUnavailable,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedStoredSecret => formatter.write_str(
                "the installation identity in the native credential store is malformed; remove it only after recovering the installation identity",
            ),
            Self::InvalidPublicIdentity => {
                formatter.write_str("public identity is not valid hexadecimal")
            }
            Self::StoreUnavailable => formatter.write_str(
                "the native credential store is unavailable; unlock or repair it before using Resonance",
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicIdentity([u8; 32]);

impl PublicIdentity {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(IdentityError::InvalidPublicIdentity);
        }
        let mut bytes = [0; 32];
        let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
        debug_assert!(remainder.is_empty());
        for (index, pair) in pairs.iter().enumerate() {
            bytes[index] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn hex_value(byte: u8) -> Result<u8, IdentityError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(IdentityError::InvalidPublicIdentity),
    }
}

impl From<[u8; 32]> for PublicIdentity {
    fn from(bytes: [u8; 32]) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<String> for PublicIdentity {
    fn from(value: String) -> Self {
        Self::parse(&value).expect("public identity must be validated before construction")
    }
}

impl From<&str> for PublicIdentity {
    fn from(value: &str) -> Self {
        Self::parse(value).expect("public identity must be validated before construction")
    }
}

impl FromStr for PublicIdentity {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Debug for PublicIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PublicIdentity")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for PublicIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityError, PublicIdentity};

    #[test]
    fn parses_only_fixed_width_hex_identities() {
        let identity = PublicIdentity::parse(&"ab".repeat(32)).expect("identity parses");
        assert_eq!(identity.as_bytes(), &[0xab; 32]);
        assert_eq!(identity.to_string(), "ab".repeat(32));
        assert_eq!(
            PublicIdentity::parse("not-an-identity"),
            Err(IdentityError::InvalidPublicIdentity)
        );
    }
}
