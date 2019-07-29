use std::fmt;
use std::hash::{Hash, Hasher};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Eq)]
enum Mixed {
    String(String),
    Byte(Vec<u8>),
    UnexpectedEOF,
}

#[derive(Clone, Eq)]
#[allow(clippy::module_name_repetitions)]
pub struct MixedString {
    data: Vec<Mixed>,
}

impl MixedString {
    pub fn from_bytes(mut input: &[u8]) -> Self {
        // https://doc.rust-lang.org/std/str/struct.Utf8Error.html#examples
        let mut res = Vec::new();
        loop {
            match ::std::str::from_utf8(input) {
                Ok(valid) => {
                    res.push(Mixed::String(valid.to_string()));
                    break;
                }
                Err(error) => {
                    let (valid, after_valid) = input.split_at(error.valid_up_to());

                    if !valid.is_empty() {
                        let utf8 = unsafe { ::std::str::from_utf8_unchecked(valid) };
                        res.push(Mixed::String(utf8.to_string()));
                    }

                    if let Some(invalid_sequence_length) = error.error_len() {
                        let b = &after_valid[..invalid_sequence_length];
                        let mut bytes = Vec::new();
                        bytes.extend_from_slice(b);

                        res.push(Mixed::Byte(bytes));
                        input = &after_valid[invalid_sequence_length..]
                    } else {
                        let mut bytes = Vec::new();
                        bytes.extend_from_slice(after_valid);
                        res.push(Mixed::Byte(bytes));
                        res.push(Mixed::UnexpectedEOF);
                        break;
                    }
                }
            }
        }
        Self { data: res }
    }

    pub fn from_string(s: String) -> Self {
        Self {
            data: vec![Mixed::String(s)],
        }
    }

    pub fn to_string(&self) -> String {
        let mut res = String::new();
        for data in &self.data {
            match data {
                Mixed::String(s) => res.push_str(s),
                Mixed::Byte(bytes) => {
                    let bytes: &[u8] = bytes;
                    for b in bytes {
                        res.push_str(&format!("\\u{{{:02x}}}", b));
                    }
                }
                Mixed::UnexpectedEOF => {
                    res.push('\u{FFDD}');
                }
            }
        }
        res
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res = Vec::new();
        for data in &self.data {
            match data {
                Mixed::String(s) => res.extend_from_slice(s.as_bytes()),
                Mixed::Byte(bytes) => res.extend_from_slice(bytes),
                Mixed::UnexpectedEOF => {}
            }
        }
        res
    }

    pub fn reverse(&mut self) {
        for data in &mut self.data {
            match data {
                Mixed::String(s) => {
                    // https://stackoverflow.com/a/27996791
                    let rev = s.graphemes(true).rev().collect();
                    *s = rev;
                }
                Mixed::Byte(b) => b.reverse(),
                Mixed::UnexpectedEOF => {}
            }
        }
        self.data.reverse();
    }
}

impl PartialEq<MixedString> for MixedString {
    fn eq(&self, other: &Self) -> bool {
        other.data == self.data
    }
}

impl PartialEq<Mixed> for Mixed {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Mixed::UnexpectedEOF => {
                if let Mixed::UnexpectedEOF = other {
                    return true;
                }
            }
            Mixed::String(s) => {
                if let Mixed::String(o) = other {
                    return s == o;
                }
            }
            Mixed::Byte(b) => {
                if let Mixed::Byte(o) = other {
                    return b == o;
                }
            }
        }
        false
    }
}

impl Hash for MixedString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash_slice(&self.data, state);
    }
}

impl Hash for Mixed {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Mixed::String(s) => Hash::hash(s, state),
            Mixed::Byte(b) => Hash::hash_slice(b, state),
            Mixed::UnexpectedEOF => Hash::hash(&3, state),
        }
    }
}

impl fmt::Display for MixedString {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.to_string().fmt(f)
    }
}

impl fmt::Debug for MixedString {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.data.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::mixed::MixedString;

    macro_rules! make_test {
        ($name:ident,$rev:ident : $input:expr) => {
            #[test]
            fn $name() {
                let (s, b, _) = $input;

                let mixed = dbg!(MixedString::from_bytes(&b));

                assert_eq!(s, &mixed.to_string());
                assert_eq!(b, &mixed.to_bytes()[..]);
            }

            #[test]
            fn $rev() {
                let (_, b, s) = $input;
                let mut b = b;

                let mut mixed = MixedString::from_bytes(&b);
                mixed.reverse();
                b.reverse();
                dbg!(&mixed);

                assert_eq!(s, &mixed.to_string());
                assert_eq!(b, &mixed.to_bytes()[..]);
            }
        };
    }

    const LETTER_A: u8 = 0x41;

    // Unicode octet never starts with 0b10xxxxxx
    const INVALID: &[u8] = &[0x80, 0x81];
    // 4-byte character starts with 0b11110xxx
    const TRUNCATED: u8 = 0xf0;

    make_test!(normal, normal_reverse: {
        let s = "Hello world!";
        let r = "!dlrow olleH";
        let b = s.as_bytes().to_vec();
        (s, b, r)
    });

    make_test!(invalid, invalid_reverse: {
        let s = r#"\u{80}\u{81}"#;
        let r = r#"\u{81}\u{80}"#;
        let b = INVALID.to_vec();
        (s, b, r)
    });

    make_test!(mixed, mixed_reverse: {
        let s = "Hello \\u{f0}A";
        let r = "A\\u{f0} olleH";

        let mut input: Vec<u8> = "Hello ".to_string().into_bytes();
        input.push(TRUNCATED);
        input.push(LETTER_A);

        (s, input, r)
    });

    make_test!(eof, eof_reverse: {
        let s = "Hello \\u{f0}\u{FFDD}";
        let r = "\u{FFDD}\\u{f0} olleH";

        let mut input: Vec<u8> = "Hello ".to_string().into_bytes();
        input.push(TRUNCATED);

        (s, input, r)
    });

    #[test]
    fn reverse_emoji() {
        let s = "123 \u{1F937}";
        let bytes = s.as_bytes().to_vec();
        assert_eq!(s.chars().count(), 4 + 1);
        assert_eq!(bytes.len(), 4 + 4);

        let mut mixed = MixedString::from_string(s.to_string());
        mixed.reverse();

        assert_eq!("\u{1F937} 321", mixed.to_string());
    }
}
