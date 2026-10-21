//! Macro names on the mouse are raw bytes in whatever code page the writing PC used (docs/ui-features.md
//! §4): the owner's are Big5. Names are read and written in the configured encoding; reading falls back
//! to the other one.

use encoding_rs::BIG5;
use hecate_core::slots::MACRO_NAME_MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameEncoding {
    Big5,
    Utf8,
}

impl NameEncoding {
    /// The `macro_name_encoding` config value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "big5" => Some(Self::Big5),
            "utf-8" => Some(Self::Utf8),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    /// Encoded length in bytes.
    TooLong(usize),
    /// The first character the encoding has no code for.
    Unencodable(char),
}

impl NameError {
    /// i18n key and argument for the pages.
    pub fn message(&self) -> (&'static str, String) {
        match self {
            NameError::Empty => ("macro.name_empty", String::new()),
            NameError::TooLong(_) => ("macro.name_too_long", MACRO_NAME_MAX.to_string()),
            NameError::Unencodable(c) => ("macro.name_unencodable", c.to_string()),
        }
    }
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NameError::Empty => write!(f, "the macro name is empty"),
            NameError::TooLong(n) => write!(f, "the macro name is {n} bytes, at most {MACRO_NAME_MAX} fit"),
            NameError::Unencodable(c) => write!(f, "'{c}' cannot be stored in the macro name encoding"),
        }
    }
}

/// Display form of stored name bytes: the configured encoding if the bytes are valid in it, else the
/// other one, else Big5 with replacement characters. Big5 first matters: some Big5 names are also valid
/// UTF-8 ("讀" = C5 AA reads as "Ū").
pub fn decode(bytes: &[u8], encoding: NameEncoding) -> String {
    let utf8 = || std::str::from_utf8(bytes).ok().map(str::to_owned);
    let big5 = || BIG5.decode_without_bom_handling_and_without_replacement(bytes).map(|s| s.into_owned());
    let strict = match encoding {
        NameEncoding::Big5 => big5().or_else(utf8),
        NameEncoding::Utf8 => utf8().or_else(big5),
    };
    strict.unwrap_or_else(|| BIG5.decode_without_bom_handling(bytes).0.into_owned())
}

/// Name bytes to store, or why the name cannot be stored.
pub fn encode(name: &str, encoding: NameEncoding) -> Result<Vec<u8>, NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    let bytes = match encoding {
        NameEncoding::Utf8 => name.as_bytes().to_vec(),
        NameEncoding::Big5 => {
            if let Some(c) = name.chars().find(|c| BIG5.encode(c.encode_utf8(&mut [0; 4])).2) {
                return Err(NameError::Unencodable(c));
            }
            BIG5.encode(name).0.into_owned()
        }
    };
    if bytes.len() > MACRO_NAME_MAX {
        return Err(NameError::TooLong(bytes.len()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner's macro names as read from slots 3 and 4.
    const BACK: [u8; 8] = [0xa4, 0xc1, 0xa6, 0x5e, 0xa5, 0x68, 0x3a, 0x44];
    const SCREEN: [u8; 8] = [0xa4, 0xc1, 0xbf, 0xc3, 0xb9, 0xf5, 0x3a, 0x44];

    #[test]
    fn decodes_big5_names_from_the_device() {
        for enc in [NameEncoding::Big5, NameEncoding::Utf8] {
            assert_eq!(decode(&BACK, enc), "切回去:D");
            assert_eq!(decode(&SCREEN, enc), "切螢幕:D");
        }
    }

    #[test]
    fn falls_back_to_the_other_encoding() {
        assert_eq!(decode("切回去".as_bytes(), NameEncoding::Big5), "切回去");
        assert_eq!(decode("切回去".as_bytes(), NameEncoding::Utf8), "切回去");
        assert_eq!(decode(b"copy", NameEncoding::Big5), "copy");
        assert_eq!(decode(b"", NameEncoding::Big5), "");
    }

    /// Review R7: Big5 names whose bytes are also valid UTF-8 read as the configured encoding.
    #[test]
    fn r7_big5_name_that_is_valid_utf8_reads_as_big5() {
        let stored = [0xC5u8, 0xAA]; // Big5 "讀"
        let shown = decode(&stored, NameEncoding::Big5);
        assert_eq!(shown, "讀");
        assert_eq!(encode(&shown, NameEncoding::Big5).unwrap(), stored);
        assert_eq!(decode(&[0xC3, 0xA9], NameEncoding::Big5), "矇");
        // With UTF-8 configured the same bytes are UTF-8.
        assert_eq!(decode(&stored, NameEncoding::Utf8), "Ū");
    }

    #[test]
    fn undecodable_bytes_still_display() {
        let s = decode(&[0xff, 0x41], NameEncoding::Big5);
        assert!(s.ends_with('A'));
        assert!(s.contains('\u{fffd}'));
    }

    #[test]
    fn encodes_big5_round_trip() {
        assert_eq!(encode("切回去:D", NameEncoding::Big5).unwrap(), BACK);
        assert_eq!(encode("切螢幕:D", NameEncoding::Big5).unwrap(), SCREEN);
        assert_eq!(encode("Copy 1", NameEncoding::Big5).unwrap(), b"Copy 1");
    }

    #[test]
    fn encodes_utf8() {
        assert_eq!(encode("切回去", NameEncoding::Utf8).unwrap(), "切回去".as_bytes());
    }

    #[test]
    fn rejects_unencodable_characters() {
        assert_eq!(encode("macro 😀", NameEncoding::Big5), Err(NameError::Unencodable('😀')));
        // Simplified-only character with no Big5 code.
        assert_eq!(encode("宏设", NameEncoding::Big5), Err(NameError::Unencodable('设')));
        assert!(encode("macro 😀", NameEncoding::Utf8).is_ok());
    }

    #[test]
    fn rejects_empty_and_too_long_names() {
        assert_eq!(encode("", NameEncoding::Big5), Err(NameError::Empty));
        assert_eq!(encode("   ", NameEncoding::Utf8), Err(NameError::Empty));
        assert_eq!(encode(&"a".repeat(30), NameEncoding::Big5).unwrap().len(), 30);
        assert_eq!(encode(&"a".repeat(31), NameEncoding::Big5), Err(NameError::TooLong(31)));
        // 15 CJK characters fit in Big5 (30 bytes) but not in UTF-8 (45 bytes).
        let cjk = "切".repeat(15);
        assert!(encode(&cjk, NameEncoding::Big5).is_ok());
        assert_eq!(encode(&cjk, NameEncoding::Utf8), Err(NameError::TooLong(45)));
    }

    #[test]
    fn encoding_config_values() {
        assert_eq!(NameEncoding::parse("big5"), Some(NameEncoding::Big5));
        assert_eq!(NameEncoding::parse("utf-8"), Some(NameEncoding::Utf8));
        assert_eq!(NameEncoding::parse("UTF8"), None);
        assert_eq!(NameEncoding::parse(""), None);
    }

    #[test]
    fn error_messages_name_the_limit_and_character() {
        assert_eq!(NameError::TooLong(31).message(), ("macro.name_too_long", "30".into()));
        assert_eq!(NameError::Unencodable('设').message(), ("macro.name_unencodable", "设".into()));
        assert!(NameError::TooLong(31).to_string().contains("31"));
    }
}
