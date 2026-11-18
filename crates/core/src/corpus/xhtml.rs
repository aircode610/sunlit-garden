//! A small, strict XHTML reader: enough XML for well-formed files such as Standard Ebooks', and an
//! error — never a guess — for anything it doesn't understand (CDATA, unknown entities, unquoted
//! attributes, tags that don't nest).
//!
//! Output is a flat token stream. A self-closing tag (`<br/>`) becomes a start tag followed by its
//! end tag, so readers never special-case it. The XML declaration, `<!DOCTYPE …>` and comments are
//! skipped.

use std::fmt;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End {
        name: String,
    },
    /// Character data with entities decoded.
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XhtmlError {
    /// Byte offset in the source where the problem was found.
    pub at: usize,
    pub message: String,
}

impl fmt::Display for XhtmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "XHTML byte {}: {}", self.at, self.message)
    }
}

impl std::error::Error for XhtmlError {}

fn err<T>(at: usize, message: impl Into<String>) -> Result<T, XhtmlError> {
    Err(XhtmlError {
        at,
        message: message.into(),
    })
}

/// Read the whole document. Fails on the first problem; on success every start tag was closed by
/// a matching end tag.
pub fn tokenize(src: &str) -> Result<Vec<Token>, XhtmlError> {
    Ok(tokenize_spans(src)?.into_iter().map(|(_, t)| t).collect())
}

/// As [`tokenize`], with the bytes of `src` each token came from: the whole tag for start and
/// end tags (a self-closing tag's start and end share it), the raw undecoded text for text.
pub fn tokenize_spans(src: &str) -> Result<Vec<(Range<usize>, Token)>, XhtmlError> {
    let b = src.as_bytes();
    let mut tokens = Vec::new();
    let mut open: Vec<String> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let end = src[i..].find('<').map_or(b.len(), |n| i + n);
            tokens.push((i..end, Token::Text(decode(&src[i..end], i)?)));
            i = end;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            i += find(rest, "-->", i)? + "-->".len();
        } else if rest.starts_with("<![CDATA[") {
            return err(i, "CDATA sections are not supported");
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            // XML declaration / processing instruction / DOCTYPE (no internal subset expected).
            let close = if rest.starts_with("<?") { "?>" } else { ">" };
            let n = find(rest, close, i)?;
            if rest[..n].contains('[') {
                return err(i, "DOCTYPE internal subsets are not supported");
            }
            i += n + close.len();
        } else if let Some(stripped) = rest.strip_prefix("</") {
            let n = find(stripped, ">", i)?;
            let name = stripped[..n].trim().to_owned();
            match open.pop() {
                Some(top) if top == name => tokens.push((i..i + 2 + n + 1, Token::End { name })),
                Some(top) => return err(i, format!("</{name}> closes <{top}>")),
                None => return err(i, format!("</{name}> with nothing open")),
            }
            i += 2 + n + 1;
        } else {
            let n = find(rest, ">", i)?;
            let inner = &rest[1..n];
            let (inner, self_closing) = match inner.strip_suffix('/') {
                Some(s) => (s, true),
                None => (inner, false),
            };
            let (name, attrs) = parse_tag(inner, i)?;
            let span = i..i + n + 1;
            tokens.push((
                span.clone(),
                Token::Start {
                    name: name.clone(),
                    attrs,
                },
            ));
            if self_closing {
                tokens.push((span, Token::End { name }));
            } else {
                open.push(name);
            }
            i += n + 1;
        }
    }
    if let Some(top) = open.pop() {
        return err(b.len(), format!("<{top}> is never closed"));
    }
    Ok(tokens)
}

/// Byte index of `needle` in `hay`, or an error at `at` if it's missing.
fn find(hay: &str, needle: &str, at: usize) -> Result<usize, XhtmlError> {
    hay.find(needle)
        .map_or_else(|| err(at, format!("missing {needle:?}")), Ok)
}

/// `name attr="v" attr2='w'` → name and decoded attributes.
fn parse_tag(inner: &str, at: usize) -> Result<(String, Vec<(String, String)>), XhtmlError> {
    let inner = inner.trim();
    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let name = &inner[..name_end];
    if name.is_empty() {
        return err(at, "tag without a name");
    }
    let mut attrs = Vec::new();
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else {
            return err(at, format!("attribute without a value in <{name}>"));
        };
        let key = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            return err(at, format!("unquoted attribute {key:?} in <{name}>"));
        };
        let Some(close) = after[1..].find(quote) else {
            return err(at, format!("unterminated attribute {key:?} in <{name}>"));
        };
        attrs.push((key.to_owned(), decode(&after[1..1 + close], at)?));
        rest = after[1 + close + 1..].trim_start();
    }
    Ok((name.to_owned(), attrs))
}

/// Decode XML's five named entities and numeric character references; refuse any other `&…;`.
pub(crate) fn decode(text: &str, at: usize) -> Result<String, XhtmlError> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let Some(semi) = rest[amp..].find(';') else {
            return err(at, "'&' without ';'");
        };
        let entity = &rest[amp + 1..amp + semi];
        let ch = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse().ok()
                } else {
                    None
                };
                match code.and_then(char::from_u32) {
                    Some(c) => c,
                    None => return err(at, format!("unknown entity &{entity};")),
                }
            }
        };
        out.push(ch);
        rest = &rest[amp + semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
