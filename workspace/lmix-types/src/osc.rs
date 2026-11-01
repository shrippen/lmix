//! An OSC message to an output target (show mode EXT.6).
//!
//! Address and arguments are text, as typed into a cue: the arguments
//! are words separated by spaces, their kind follows from how they look.
//!
//! ```text
//!   /eos/chan/1/at   50 0.5 "Szene 2" true nil
//!                    │  │   │         │    └─ nil
//!                    │  │   │         └─ true / false
//!                    │  │   └─ text (quoted: with spaces; \" and \\ inside)
//!                    │  └─ float (with a point or an exponent)
//!                    └─ int (32 bit)
//! ```
//!
//! Any other word is text too (`go` is `"go"`). A whole number beyond
//! 32 bit, or a decimal a 32-bit float cannot hold (`1e99`, `1e-50`), is
//! refused rather than changed; so are control characters. The address is
//! printable ASCII (OSC 1.0). Both are checked when an action is read, so
//! a cue never holds a message that cannot be sent.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Longest address, in bytes.
pub const OSC_ADDRESS_MAX: usize = 256;

/// Most arguments of one message.
pub const OSC_ARGS_MAX: usize = 32;

/// Longest argument text, in bytes (far below a UDP datagram).
pub const OSC_ARGS_TEXT_MAX: usize = 4096;

/// Characters the OSC address pattern reserves (OSC 1.0) besides `/`.
const RESERVED: [char; 3] = [' ', '#', ','];

/// One argument.
#[derive(Clone, Debug, PartialEq)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Text(String),
    Bool(bool),
    Nil,
}

/// An address such as `/eos/chan/1/at`: starts with `/`, no spaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OscAddress(String);

impl OscAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OscAddress {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if !text.starts_with('/') {
            return Err(format!("OSC address {text:?} does not start with /"));
        }
        if text.len() > OSC_ADDRESS_MAX {
            return Err(format!("OSC address longer than {OSC_ADDRESS_MAX} bytes"));
        }
        let bad = |c: char| !c.is_ascii_graphic() || RESERVED.contains(&c);
        if let Some(c) = text.chars().find(|c| bad(*c)) {
            return Err(format!("OSC address {text:?} holds {c:?}"));
        }
        Ok(Self(text))
    }
}

impl From<OscAddress> for String {
    fn from(address: OscAddress) -> Self {
        address.0
    }
}

/// The arguments as typed, e.g. `50 0.5 "Szene 2"`; empty for none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OscArgs(String);

impl OscArgs {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The arguments; the text was checked when it was read.
    pub fn args(&self) -> Vec<OscArg> {
        parse_args(&self.0).unwrap_or_default()
    }
}

impl TryFrom<String> for OscArgs {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        if text.len() > OSC_ARGS_TEXT_MAX {
            return Err(format!(
                "OSC arguments longer than {OSC_ARGS_TEXT_MAX} bytes"
            ));
        }
        parse_args(&text)?;
        Ok(Self(text))
    }
}

impl From<OscArgs> for String {
    fn from(args: OscArgs) -> Self {
        args.0
    }
}

/// `text` as arguments, or why not.
pub fn parse_args(text: &str) -> Result<Vec<OscArg>, String> {
    let mut args = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(&first) = chars.peek() else {
            return Ok(args);
        };
        if args.len() == OSC_ARGS_MAX {
            return Err(format!("at most {OSC_ARGS_MAX} OSC arguments"));
        }
        if first == '"' {
            chars.next();
            args.push(OscArg::Text(quoted(&mut chars)?));
            continue;
        }
        let mut word = String::new();
        while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
            if c == '"' {
                return Err(format!("a quote inside {word}\": put the text in quotes"));
            }
            if c.is_control() {
                return Err(format!("a control character {c:?} in {word}"));
            }
            word.push(c);
        }
        args.push(word_arg(word)?);
    }
}

/// The rest of a quoted text, after its opening quote.
fn quoted(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String, String> {
    let mut text = String::new();
    loop {
        match chars.next() {
            None => return Err("a quote is not closed".into()),
            Some('"') => break,
            Some('\\') => match chars.next() {
                Some(c @ ('"' | '\\')) => text.push(c),
                _ => return Err("only \\\" and \\\\ inside quotes".into()),
            },
            Some(c) if c.is_control() => {
                return Err(format!("a control character {c:?} in \"{text}"));
            }
            Some(c) => text.push(c),
        }
    }
    if chars.peek().is_some_and(|c| !c.is_whitespace()) {
        return Err(format!("a space after \"{text}\""));
    }
    Ok(text)
}

/// An unquoted word: a number, `true`, `false`, `nil`, or text.
fn word_arg(word: String) -> Result<OscArg, String> {
    match word.as_str() {
        "true" => return Ok(OscArg::Bool(true)),
        "false" => return Ok(OscArg::Bool(false)),
        "nil" => return Ok(OscArg::Nil),
        _ => {}
    }
    let digits = word.strip_prefix(['+', '-']).unwrap_or(&word);
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return word
            .parse::<i32>()
            .map(OscArg::Int)
            .map_err(|_| format!("{word} is beyond a 32-bit whole number; {word}.0 is a float"));
    }
    let numeric = word
        .chars()
        .all(|c| c.is_ascii_digit() || "+-.eE".contains(c));
    let Some(wide) = word.parse::<f64>().ok().filter(|_| numeric) else {
        return Ok(OscArg::Text(word));
    };
    let float = wide as f32;
    if !float.is_finite() {
        return Err(format!("{word} is too big for a 32-bit float"));
    }
    if float == 0.0 && wide != 0.0 {
        return Err(format!("{word} is too small for a 32-bit float"));
    }
    Ok(OscArg::Float(float))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_take_their_kind_from_how_they_look() {
        let args = parse_args(r#" 50 -3 0.5 1e3 "Szene 2" true false nil go "" "#).unwrap();
        assert_eq!(
            args,
            vec![
                OscArg::Int(50),
                OscArg::Int(-3),
                OscArg::Float(0.5),
                OscArg::Float(1000.0),
                OscArg::Text("Szene 2".into()),
                OscArg::Bool(true),
                OscArg::Bool(false),
                OscArg::Nil,
                OscArg::Text("go".into()),
                OscArg::Text(String::new()),
            ]
        );
    }

    #[test]
    fn odd_words_stay_text() {
        // Names, not numbers.
        let args = parse_args("inf NaN 1.2.3 - 3000000000.0").unwrap();
        assert_eq!(args[0], OscArg::Text("inf".into()));
        assert_eq!(args[1], OscArg::Text("NaN".into()));
        assert_eq!(args[2], OscArg::Text("1.2.3".into()));
        assert_eq!(args[3], OscArg::Text("-".into()));
        assert_eq!(args[4], OscArg::Float(3.0e9));
    }

    #[test]
    fn numbers_a_receiver_would_misread_are_refused() {
        // Beyond 32 bit, too big or too small for a float: never changed.
        for word in ["3000000000", "-2147483649", "1e99", "1e-50"] {
            assert!(parse_args(word).is_err(), "{word}");
        }
        assert_eq!(parse_args("-2147483648").unwrap(), [OscArg::Int(i32::MIN)]);
    }

    #[test]
    fn control_characters_are_refused() {
        // A NUL would end the OSC string early and shift every argument after.
        for text in ["\"ab\0cd\" 50", "a\0b", "\"a\u{7}\"", "\"a\tb\""] {
            assert!(parse_args(text).is_err(), "{text:?}");
        }
        assert_eq!(parse_args("a\tb").unwrap().len(), 2, "a tab separates");
    }

    #[test]
    fn quotes_take_escapes_and_must_close() {
        let args = parse_args(r#""a \"b\" \\ c""#).unwrap();
        assert_eq!(args, vec![OscArg::Text(r#"a "b" \ c"#.into())]);
        assert!(parse_args(r#""open"#).is_err());
        assert!(parse_args(r#""a"b"#).is_err());
        assert!(parse_args(r#"a"b""#).is_err());
        assert!(parse_args(r#""\n""#).is_err());
        assert_eq!(parse_args("").unwrap(), vec![]);
    }

    #[test]
    fn at_most_the_limit() {
        let many = vec!["1"; OSC_ARGS_MAX].join(" ");
        assert_eq!(parse_args(&many).unwrap().len(), OSC_ARGS_MAX);
        assert!(parse_args(&format!("{many} 1")).is_err());
        let long = "x".repeat(OSC_ARGS_TEXT_MAX + 1);
        assert!(OscArgs::try_from(long).is_err());
    }

    #[test]
    fn addresses_start_with_a_slash_and_hold_no_spaces() {
        assert!(OscAddress::try_from("/eos/chan/1/at".to_string()).is_ok());
        for bad in ["eos", "", "/a b", "/a#b", "/a,b", "/a\tb", "/grün", "/a\0"] {
            assert!(OscAddress::try_from(bad.to_string()).is_err(), "{bad}");
        }
        let long = format!("/{}", "a".repeat(OSC_ADDRESS_MAX));
        assert!(OscAddress::try_from(long).is_err());
    }

    /// Outside input (API, show file): random text never panics, and
    /// what reads once reads the same again. Seeded, so a failure repeats.
    #[test]
    fn random_text_never_panics() {
        const ALPHABET: &[char] = &[
            ' ', '"', '\\', '1', '0', '.', '-', '+', 'e', 'E', 't', 'r', 'u', 'n', 'i', 'l', 'x',
            '/', '#', ',', '\t', 'ü', '\0',
        ];
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let len = (next() % 24) as usize;
            let text: String = (0..len)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
                .collect();
            let _ = OscAddress::try_from(text.clone());
            if let Ok(args) = OscArgs::try_from(text.clone()) {
                assert_eq!(Ok(args.args()), parse_args(&text), "{text:?}");
                let texts = args.args().into_iter().filter_map(|a| match a {
                    OscArg::Text(t) => Some(t),
                    _ => None,
                });
                for t in texts {
                    assert!(!t.chars().any(char::is_control), "{text:?}");
                }
            }
        }
    }

    #[test]
    fn the_schema_says_text() {
        for schema in [
            schemars::schema_for!(OscArgs),
            schemars::schema_for!(OscAddress),
        ] {
            assert_eq!(schema.get("type"), Some(&serde_json::json!("string")));
        }
    }

    #[test]
    fn json_round_trip_keeps_the_text() {
        let args: OscArgs = serde_json::from_str(r#""1  2.0 \"x\"""#).unwrap();
        assert_eq!(args.as_str(), r#"1  2.0 "x""#);
        assert_eq!(serde_json::to_string(&args).unwrap(), r#""1  2.0 \"x\"""#);
        assert!(serde_json::from_str::<OscArgs>(r#""\"open""#).is_err());
        assert!(serde_json::from_str::<OscAddress>(r#""no/slash""#).is_err());
    }
}
