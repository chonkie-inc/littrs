//! Parsing of string literals, bytes literals, and implicit string concatenation.

use bstr::ByteSlice;
use std::fmt;

use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, AnyStringFlags, AtomicNodeIndex, Expr, StringFlags};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::error::{LexicalError, LexicalErrorType};

#[derive(Debug)]
pub(crate) enum StringType {
    Str(ast::StringLiteral),
    Bytes(ast::BytesLiteral),
    FString(ast::FString),
    TString(ast::TString),
}

impl Ranged for StringType {
    fn range(&self) -> TextRange {
        match self {
            Self::Str(node) => node.range(),
            Self::Bytes(node) => node.range(),
            Self::FString(node) => node.range(),
            Self::TString(node) => node.range(),
        }
    }
}

impl From<StringType> for Expr {
    fn from(string: StringType) -> Self {
        match string {
            StringType::Str(node) => Expr::from(node),
            StringType::Bytes(node) => Expr::from(node),
            StringType::FString(node) => Expr::from(node),
            StringType::TString(node) => Expr::from(node),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterpolatedStringKind {
    FString,
    TString,
}

impl InterpolatedStringKind {
    #[inline]
    pub(crate) const fn start_token(self) -> TokenKind {
        match self {
            InterpolatedStringKind::FString => TokenKind::FStringStart,
            InterpolatedStringKind::TString => TokenKind::TStringStart,
        }
    }

    #[inline]
    pub(crate) const fn middle_token(self) -> TokenKind {
        match self {
            InterpolatedStringKind::FString => TokenKind::FStringMiddle,
            InterpolatedStringKind::TString => TokenKind::TStringMiddle,
        }
    }

    #[inline]
    pub(crate) const fn end_token(self) -> TokenKind {
        match self {
            InterpolatedStringKind::FString => TokenKind::FStringEnd,
            InterpolatedStringKind::TString => TokenKind::TStringEnd,
        }
    }
}

impl fmt::Display for InterpolatedStringKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InterpolatedStringKind::FString => f.write_str("f-string"),
            InterpolatedStringKind::TString => f.write_str("t-string"),
        }
    }
}

enum EscapedChar {
    Literal(char),
    Escape(char),
}

struct StringParser {
    /// The raw content of the string e.g., the `foo` part in `"foo"`.
    source: Box<str>,
    /// Current position of the parser in the source.
    cursor: usize,
    /// Flags that can be used to query information about the string.
    flags: AnyStringFlags,
    /// The location of the first character in the source from the start of the file.
    offset: TextSize,
    /// The range of the string literal.
    range: TextRange,
}

impl StringParser {
    fn new(source: Box<str>, flags: AnyStringFlags, offset: TextSize, range: TextRange) -> Self {
        Self {
            source,
            cursor: 0,
            flags,
            offset,
            range,
        }
    }

    #[inline]
    fn skip_bytes(&mut self, bytes: usize) -> &str {
        let skipped_str = &self.source[self.cursor..self.cursor + bytes];
        self.cursor += bytes;
        skipped_str
    }

    /// Returns the current position of the parser considering the offset.
    #[inline]
    fn position(&self) -> TextSize {
        self.compute_position(self.cursor)
    }

    /// Computes the position of the cursor considering the offset.
    #[inline]
    fn compute_position(&self, cursor: usize) -> TextSize {
        self.offset + TextSize::try_from(cursor).unwrap()
    }

    /// Returns the next byte in the string, if there is one.
    ///
    /// # Panics
    ///
    /// When the next byte is a part of a multi-byte character.
    #[inline]
    fn next_byte(&mut self) -> Option<u8> {
        self.source[self.cursor..].as_bytes().first().map(|&byte| {
            self.cursor += 1;
            byte
        })
    }

    #[inline]
    fn next_char(&mut self) -> Option<char> {
        self.source[self.cursor..].chars().next().inspect(|c| {
            self.cursor += c.len_utf8();
        })
    }

    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.source[self.cursor..].as_bytes().first().copied()
    }

    fn parse_unicode_literal(&mut self, literal_number: usize) -> Result<char, LexicalError> {
        let mut p: u32 = 0u32;
        for i in 1..=literal_number {
            let start = self.position();
            match self.next_char() {
                Some(c) => match c.to_digit(16) {
                    Some(d) => p += d << ((literal_number - i) * 4),
                    None => {
                        return Err(LexicalError::new(
                            LexicalErrorType::UnicodeError,
                            TextRange::at(start, TextSize::try_from(c.len_utf8()).unwrap()),
                        ));
                    }
                },
                None => {
                    return Err(LexicalError::new(
                        LexicalErrorType::UnicodeError,
                        TextRange::empty(self.position()),
                    ));
                }
            }
        }
        match p {
            0xD800..=0xDFFF => Ok(std::char::REPLACEMENT_CHARACTER),
            _ => std::char::from_u32(p).ok_or(LexicalError::new(
                LexicalErrorType::UnicodeError,
                TextRange::empty(self.position()),
            )),
        }
    }

    fn parse_octet(&mut self, o: u8) -> char {
        let mut radix_bytes = [o, 0, 0];
        let mut len = 1;

        while len < 3 {
            let Some(b'0'..=b'7') = self.peek_byte() else {
                break;
            };

            radix_bytes[len] = self.next_byte().unwrap();
            len += 1;
        }

        // OK because radix_bytes is always going to be in the ASCII range.
        let radix_str = std::str::from_utf8(&radix_bytes[..len]).expect("ASCII bytes");
        let value = u32::from_str_radix(radix_str, 8).unwrap();
        char::from_u32(value).unwrap()
    }

    fn parse_unicode_name(&mut self) -> Result<char, LexicalError> {
        let start_pos = self.position();
        let Some('{') = self.next_char() else {
            return Err(LexicalError::new(
                LexicalErrorType::MissingUnicodeLbrace,
                TextRange::empty(start_pos),
            ));
        };

        let start_pos = self.position();
        let Some(close_idx) = self.source[self.cursor..].find('}') else {
            return Err(LexicalError::new(
                LexicalErrorType::MissingUnicodeRbrace,
                TextRange::empty(self.compute_position(self.source.len())),
            ));
        };

        let name_and_ending = self.skip_bytes(close_idx + 1);
        let name = &name_and_ending[..name_and_ending.len() - 1];

        unicode_names2::character(name).ok_or_else(|| {
            LexicalError::new(
                LexicalErrorType::UnicodeError,
                // The cursor is right after the `}` character, so we subtract 1 to get the correct
                // range of the unicode name.
                TextRange::new(
                    start_pos,
                    self.compute_position(self.cursor - '}'.len_utf8()),
                ),
            )
        })
    }

    /// Parse an escaped character, returning the new character.
    fn parse_escaped_char(&mut self) -> Result<Option<EscapedChar>, LexicalError> {
        let Some(first_char) = self.next_char() else {
            // TODO: check when this error case happens
            return Err(LexicalError::new(
                LexicalErrorType::StringError,
                TextRange::empty(self.position()),
            ));
        };

        let new_char = match first_char {
            '\\' => '\\',
            '\'' => '\'',
            '\"' => '"',
            'a' => '\x07',
            'b' => '\x08',
            'f' => '\x0c',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\x0b',
            o @ '0'..='7' => self.parse_octet(o as u8),
            'x' => self.parse_unicode_literal(2)?,
            'u' if !self.flags.is_byte_string() => self.parse_unicode_literal(4)?,
            'U' if !self.flags.is_byte_string() => self.parse_unicode_literal(8)?,
            'N' if !self.flags.is_byte_string() => self.parse_unicode_name()?,
            // Special cases where the escape sequence is not a single character
            '\n' => return Ok(None),
            '\r' => {
                if self.peek_byte() == Some(b'\n') {
                    self.next_byte();
                }

                return Ok(None);
            }
            _ => return Ok(Some(EscapedChar::Escape(first_char))),
        };

        Ok(Some(EscapedChar::Literal(new_char)))
    }

    fn parse_interpolated_string_middle(
        mut self,
    ) -> Result<ast::InterpolatedStringLiteralElement, LexicalError> {
        // Fast-path: if the f-string or t-string doesn't contain any escape sequences, return the literal.
        let Some(mut index) = memchr::memchr3(b'{', b'}', b'\\', self.source.as_bytes()) else {
            return Ok(ast::InterpolatedStringLiteralElement {
                value: self.source,
                range: self.range,
                node_index: AtomicNodeIndex::NONE,
            });
        };

        let mut value = String::with_capacity(self.source.len());
        loop {
            // Add the characters before the escape sequence (or curly brace) to the string.
            let before_with_slash_or_brace = self.skip_bytes(index + 1);
            let before = &before_with_slash_or_brace[..before_with_slash_or_brace.len() - 1];
            value.push_str(before);

            // Add the escaped character to the string.
            match &self.source.as_bytes()[self.cursor - 1] {
                // If there are any curly braces inside a `F/TStringMiddle` token,
                // then they were escaped (i.e. `{{` or `}}`). This means that
                // we need increase the location by 2 instead of 1.
                b'{' => {
                    self.offset += TextSize::from(1);
                    value.push('{');
                }
                b'}' => {
                    self.offset += TextSize::from(1);
                    value.push('}');
                }
                // We can encounter a `\` as the last character in a `F/TStringMiddle`
                // token which is valid in this context. For example,
                //
                // ```python
                // f"\{foo} \{bar:\}"
                // # ^     ^^     ^
                // ```
                //
                // Here, the `F/TStringMiddle` token content will be "\" and " \"
                // which is invalid if we look at the content in isolation:
                //
                // ```python
                // "\"
                // ```
                //
                // However, the content is syntactically valid in the context of
                // the f/t-string because it's a substring of the entire f/t-string.
                // This is still an invalid escape sequence, but we don't want to
                // raise a syntax error as is done by the CPython parser. It might
                // be supported in the future, refer to point 3: https://peps.python.org/pep-0701/#rejected-ideas
                b'\\' => {
                    if !self.flags.is_raw_string() && self.peek_byte().is_some() {
                        match self.parse_escaped_char()? {
                            None => {}
                            Some(EscapedChar::Literal(c)) => value.push(c),
                            Some(EscapedChar::Escape(c)) => {
                                value.push('\\');
                                value.push(c);
                            }
                        }
                    } else {
                        value.push('\\');
                    }
                }
                ch => {
                    unreachable!("Expected '{{', '}}', or '\\' but got {:?}", ch);
                }
            }

            let Some(next_index) =
                memchr::memchr3(b'{', b'}', b'\\', self.source[self.cursor..].as_bytes())
            else {
                // Add the rest of the string to the value.
                let rest = &self.source[self.cursor..];
                value.push_str(rest);
                break;
            };

            index = next_index;
        }

        Ok(ast::InterpolatedStringLiteralElement {
            value: value.into_boxed_str(),
            range: self.range,
            node_index: AtomicNodeIndex::NONE,
        })
    }

    fn parse_bytes(mut self) -> Result<StringType, LexicalError> {
        if let Some(index) = self.source.as_bytes().find_non_ascii_byte() {
            let ch = self.source.chars().nth(index).unwrap();
            return Err(LexicalError::new(
                LexicalErrorType::InvalidByteLiteral,
                TextRange::at(
                    self.compute_position(index),
                    TextSize::try_from(ch.len_utf8()).unwrap(),
                ),
            ));
        }

        if self.flags.is_raw_string() {
            // For raw strings, no escaping is necessary.
            return Ok(StringType::Bytes(ast::BytesLiteral {
                value: self.source.into_boxed_bytes(),
                range: self.range,
                flags: self.flags.into(),
                node_index: AtomicNodeIndex::NONE,
            }));
        }

        let Some(mut escape) = memchr::memchr(b'\\', self.source.as_bytes()) else {
            // If the string doesn't contain any escape sequences, return the owned string.
            return Ok(StringType::Bytes(ast::BytesLiteral {
                value: self.source.into_boxed_bytes(),
                range: self.range,
                flags: self.flags.into(),
                node_index: AtomicNodeIndex::NONE,
            }));
        };

        // If the string contains escape sequences, we need to parse them.
        let mut value = Vec::with_capacity(self.source.len());
        loop {
            // Add the characters before the escape sequence to the string.
            let before_with_slash = self.skip_bytes(escape + 1);
            let before = &before_with_slash[..before_with_slash.len() - 1];
            value.extend_from_slice(before.as_bytes());

            // Add the escaped character to the string.
            match self.parse_escaped_char()? {
                None => {}
                Some(EscapedChar::Literal(c)) => value.push(c as u8),
                Some(EscapedChar::Escape(c)) => {
                    value.push(b'\\');
                    value.push(c as u8);
                }
            }

            let Some(next_escape) = memchr::memchr(b'\\', self.source[self.cursor..].as_bytes())
            else {
                // Add the rest of the string to the value.
                let rest = &self.source[self.cursor..];
                value.extend_from_slice(rest.as_bytes());
                break;
            };

            // Update the position of the next escape sequence.
            escape = next_escape;
        }

        Ok(StringType::Bytes(ast::BytesLiteral {
            value: value.into_boxed_slice(),
            range: self.range,
            flags: self.flags.into(),
            node_index: AtomicNodeIndex::NONE,
        }))
    }

    fn parse_string(mut self) -> Result<StringType, LexicalError> {
        if self.flags.is_raw_string() {
            // For raw strings, no escaping is necessary.
            return Ok(StringType::Str(ast::StringLiteral {
                value: self.source,
                range: self.range,
                flags: self.flags.into(),
                node_index: AtomicNodeIndex::NONE,
            }));
        }

        let Some(mut escape) = memchr::memchr(b'\\', self.source.as_bytes()) else {
            // If the string doesn't contain any escape sequences, return the owned string.
            return Ok(StringType::Str(ast::StringLiteral {
                value: self.source,
                range: self.range,
                flags: self.flags.into(),
                node_index: AtomicNodeIndex::NONE,
            }));
        };

        // If the string contains escape sequences, we need to parse them.
        let mut value = String::with_capacity(self.source.len());

        loop {
            // Add the characters before the escape sequence to the string.
            let before_with_slash = self.skip_bytes(escape + 1);
            let before = &before_with_slash[..before_with_slash.len() - 1];
            value.push_str(before);

            // Add the escaped character to the string.
            match self.parse_escaped_char()? {
                None => {}
                Some(EscapedChar::Literal(c)) => value.push(c),
                Some(EscapedChar::Escape(c)) => {
                    value.push('\\');
                    value.push(c);
                }
            }

            let Some(next_escape) = self.source[self.cursor..].find('\\') else {
                // Add the rest of the string to the value.
                let rest = &self.source[self.cursor..];
                value.push_str(rest);
                break;
            };

            // Update the position of the next escape sequence.
            escape = next_escape;
        }

        Ok(StringType::Str(ast::StringLiteral {
            value: value.into_boxed_str(),
            range: self.range,
            flags: self.flags.into(),
            node_index: AtomicNodeIndex::NONE,
        }))
    }

    fn parse(self) -> Result<StringType, LexicalError> {
        if self.flags.is_byte_string() {
            self.parse_bytes()
        } else {
            self.parse_string()
        }
    }
}

pub(crate) fn parse_string_literal(
    source: Box<str>,
    flags: AnyStringFlags,
    range: TextRange,
) -> Result<StringType, LexicalError> {
    StringParser::new(source, flags, range.start() + flags.opener_len(), range).parse()
}

// TODO(dhruvmanila): Move this to the new parser
pub(crate) fn parse_interpolated_string_literal_element(
    source: Box<str>,
    flags: AnyStringFlags,
    range: TextRange,
) -> Result<ast::InterpolatedStringLiteralElement, LexicalError> {
    StringParser::new(source, flags, range.start(), range).parse_interpolated_string_middle()
}
