use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Version([u32; 3]);
impl FromStr for Version {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let parts = text.split('.').collect::<Vec<_>>();
        if parts.len() != 3
            || parts.iter().any(|p| {
                p.is_empty()
                    || p.len() > 10
                    || !p.bytes().all(|b| b.is_ascii_digit())
                    || (p.len() > 1 && p.starts_with('0'))
            })
        {
            return Err("Unsupported update version".into());
        }
        let mut version = [0; 3];
        for (index, part) in parts.iter().enumerate() {
            version[index] = part.parse().map_err(|_| "Update version is too large")?;
        }
        Ok(Self(version))
    }
}
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0[0], self.0[1], self.0[2])
    }
}
#[derive(Clone, Debug)]
pub struct Manifest {
    pub version: Version,
    pub size: u64,
    pub hash: String,
    pub schema: i64,
    pub notes: String,
}
impl Manifest {
    pub fn verified(bytes: &[u8], signature: &[u8]) -> Result<Self, String> {
        super::crypto::verify(bytes, signature)?;
        Self::parse(bytes)
    }
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "Update metadata is not UTF-8")?;
        let table = text
            .parse::<toml::Table>()
            .map_err(|_| "Invalid update metadata")?;
        let string = |key| {
            table
                .get(key)
                .and_then(toml::Value::as_str)
                .ok_or("Missing update field")
        };
        let integer = |key| {
            table
                .get(key)
                .and_then(toml::Value::as_integer)
                .ok_or("Missing update field")
        };
        if integer("format")? != 1 {
            return Err("Unsupported update format".into());
        }
        let version = string("version")?.parse()?;
        let size = integer("size")?;
        let hash = string("sha256")?.to_owned();
        let schema = integer("schema")?;
        let notes = string("notes")?.to_owned();
        if !(65_536..=128 * 1024 * 1024).contains(&size)
            || !(1..=10_000).contains(&schema)
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || notes.len() > 4000
            || notes.contains('\0')
        {
            return Err("Invalid update size, checksum, schema or notes".into());
        }
        Ok(Self {
            version,
            size: size as u64,
            hash,
            schema,
            notes,
        })
    }
    pub fn asset_url(&self) -> String {
        format!(
            "https://github.com/HenkDz/clipdeck/releases/download/v{0}/clipdeck-{0}-windows-x64.exe",
            self.version
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_order_numerically_and_cannot_supply_paths() {
        assert!("0.1.10".parse::<Version>().unwrap() > "0.1.9".parse().unwrap());
        for bad in [
            "v0.1.2",
            "0.1.2-beta",
            "0.1.02",
            "../1.2",
            "1.2",
            "1.2.3/evil",
            "4294967296.0.0",
        ] {
            assert!(bad.parse::<Version>().is_err(), "{bad}");
        }
    }
    #[test]
    fn rejects_oversize_downloads_and_malformed_metadata() {
        let data = include_str!("../../assets/update-test.toml");
        assert!(Manifest::parse(data.as_bytes()).is_ok());
        for changed in [
            data.replace("65536", "-1"),
            data.replace("65536", "134217729"),
            data.replace("format = 1", "format = 2"),
            data.replace("9.8.7", "../../evil"),
        ] {
            assert!(Manifest::parse(changed.as_bytes()).is_err());
        }
    }
}
