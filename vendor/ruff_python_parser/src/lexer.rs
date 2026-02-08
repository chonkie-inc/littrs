//! This module takes care of lexing Python source text.
//!
//! This means source code is scanned and translated into separate tokens. The rules
//! governing what is and is not a valid token are defined in the Python reference
//! guide section on [Lexical analysis].
//!
//! [Lexical analysis]: https://docs.python.org/3/reference/lexical_analysis.html

use std::cmp::Ordering;
use std::str::FromStr;

use unicode_ident::{is_xid_continue, is_xid_start};
use unicode_normalization::UnicodeNormalization;

use ruff_python_ast::name::Name;
use ruff_python_ast::str_prefix::{AnyStringPrefix, StringLiteralPrefix};
use ruff_python_ast::token::{TokenFlags, TokenKind};
use ruff_python_ast::{Int, IpyEscapeKind, StringFlags};
use ruff_python_trivia::is_python_whitespace;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::Mode;
use crate::error::{InterpolatedStringErrorType, LexicalError, LexicalErrorType};
use crate::lexer::cursor::{Cursor, EOF_CHAR};
use crate::lexer::indentation::{Indentation, Indentations, IndentationsCheckpoint};
use crate::lexer::interpolated_string::{
    InterpolatedStringContext, InterpolatedStrings, InterpolatedStringsCheckpoint,
};
use crate::string::InterpolatedStringKind;
use crate::token::TokenValue;

mod cursor;
mod indentation;
mod interpolated_string;

const BOM: char = '\u{feff}';

/// A lexer for Python source code.
#[derive(Debug)]
pub struct Lexer<'src> {
    /// Source code to be lexed.
    source: &'src str,

    /// A pointer to the current character of the source code which is being lexed.
    cursor: Cursor<'src>,

    /// The kind of the current token.
    current_kind: TokenKind,

    /// The range of the current token.
    current_range: TextRange,

    /// The value of the current token.
    current_value: TokenValue,

    /// Flags for the current token.
    current_flags: TokenFlags,

    /// Lexer state.
    state: State,

    /// Represents the current level of nesting in the lexer, indicating the depth of parentheses.
    /// The lexer is within a parenthesized context if the value is greater than 0.
    nesting: u32,

    /// A stack of indentation representing the current indentation level.
    indentations: Indentations,
    pending_indentation: Option<Indentation>,

    /// Lexer mode.
    mode: Mode,

    /// F-string and t-string contexts.
    interpolated_strings: InterpolatedStrings,

    /// Errors encountered while lexing.
    errors: Vec<LexicalError>,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given input source which starts at the given offset.
    ///
    /// If the start offset is greater than 0, the cursor is moved ahead that many bytes.
    /// This means that the input source should be the complete source code and not the
    /// sliced version.
    pub(crate) fn new(source: &'src str, mode: Mode, start_offset: TextSize) -> Self {
        assert!(
            u32::try_from(source.len()).is_ok(),
            "Lexer only supports files with a size up to 4GB"
        );

        let (state, nesting) = if mode == Mode::ParenthesizedExpression {
            (State::Other, 1)
        } else {
            (State::AfterNewline, 0)
        };

        let mut lexer = Lexer {
            source,
            cursor: Cursor::new(source),
            state,
            current_kind: TokenKind::EndOfFile,
            current_range: TextRange::empty(start_offset),
            current_value: TokenValue::None,
            current_flags: TokenFlags::empty(),
            nesting,
            indentations: Indentations::default(),
            pending_indentation: None,
            mode,
            interpolated_strings: InterpolatedStrings::default(),
            errors: Vec::new(),
        };

        if start_offset == TextSize::new(0) {
            // TODO: Handle possible mismatch between BOM and explicit encoding declaration.
            lexer.cursor.eat_char(BOM);
        } else {
            lexer.cursor.skip_bytes(start_offset.to_usize());
        }

        lexer
    }

    /// Returns the kind of the current token.
    pub(crate) fn current_kind(&self) -> TokenKind {
        self.current_kind
    }

    /// Returns the range of the current token.
    pub(crate) fn current_range(&self) -> TextRange {
        self.current_range
    }

    /// Returns the flags for the current token.
    pub(crate) fn current_flags(&self) -> TokenFlags {
        self.current_flags
    }

    /// Takes the token value corresponding to the current token out of the lexer, replacing it
    /// with the default value.
    ///
    /// All the subsequent call to this method without moving the lexer would always return the
    /// default value which is [`TokenValue::None`].
    pub(crate) fn take_value(&mut self) -> TokenValue {
        std::mem::take(&mut self.current_value)
    }

    /// Helper function to push the given error, updating the current range with the error location
    /// and return the [`TokenKind::Unknown`] token.
    fn push_error(&mut self, error: LexicalError) -> TokenKind {
        self.current_range = error.location();
        self.errors.push(error);
        TokenKind::Unknown
    }

    /// Lex the next token.
    pub fn next_token(&mut self) -> TokenKind {
        self.cursor.start_token();
        self.current_value = TokenValue::None;
        self.current_flags = TokenFlags::empty();
        self.current_kind = self.lex_token();
        // For `Unknown` token, the `push_error` method updates the current range.
        if !matches!(self.current_kind, TokenKind::Unknown) {
            self.current_range = self.token_range();
        }
        self.current_kind
    }

    fn lex_token(&mut self) -> TokenKind {
        if let Some(interpolated_string) = self.interpolated_strings.current() {
            if !interpolated_string.is_in_interpolation(self.nesting) {
                if let Some(token) = self.lex_interpolated_string_middle_or_end() {
                    if token.is_interpolated_string_end() {
                        self.interpolated_strings.pop();
                    }
                    return token;
                }
            }
        }
        // Return dedent tokens until the current indentation level matches the indentation of the next token.
        else if let Some(indentation) = self.pending_indentation.take() {
            match self.indentations.current().try_compare(indentation) {
                Ok(Ordering::Greater) => {
                    self.pending_indentation = Some(indentation);
                    if self.indentations.dedent_one(indentation).is_err() {
                        return self.push_error(LexicalError::new(
                            LexicalErrorType::IndentationError,
                            self.token_range(),
                        ));
                    }
                    return TokenKind::Dedent;
                }
                Ok(_) => {}
                Err(_) => {
                    return self.push_error(LexicalError::new(
                        LexicalErrorType::IndentationError,
                        self.token_range(),
                    ));
                }
            }
        }

        if self.state.is_after_newline() {
            if let Some(indentation) = self.eat_indentation() {
                return indentation;
            }
        } else {
            if let Err(error) = self.skip_whitespace() {
                return self.push_error(error);
            }
        }

        // The lexer might've skipped whitespaces, so update the start offset
        self.cursor.start_token();

        if let Some(c) = self.cursor.bump() {
            if c.is_ascii() {
                self.consume_ascii_character(c)
            } else if is_unicode_identifier_start(c) {
                let identifier = self.lex_identifier(c);
                self.state = State::Other;

                identifier
            } else {
                self.push_error(LexicalError::new(
                    LexicalErrorType::UnrecognizedToken { tok: c },
                    self.token_range(),
                ))
            }
        } else {
            // Reached the end of the file. Emit a trailing newline token if not at the beginning of a logical line,
            // empty the dedent stack, and finally, return the EndOfFile token.
            self.consume_end()
        }
    }

    fn eat_indentation(&mut self) -> Option<TokenKind> {
        let mut indentation = Indentation::root();

        loop {
            match self.cursor.first() {
                ' ' => {
                    self.cursor.bump();
                    indentation = indentation.add_space();
                }
                '\t' => {
                    self.cursor.bump();
                    indentation = indentation.add_tab();
                }
                '\\' => {
                    self.cursor.bump();
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else if !self.cursor.eat_char('\n') {
                        return Some(self.push_error(LexicalError::new(
                            LexicalErrorType::LineContinuationError,
                            TextRange::at(self.offset() - '\\'.text_len(), '\\'.text_len()),
                        )));
                    }
                    if self.cursor.is_eof() {
                        return Some(self.push_error(LexicalError::new(
                            LexicalErrorType::Eof,
                            self.token_range(),
                        )));
                    }
                    indentation = Indentation::root();
                }
                // Form feed
                '\x0C' => {
                    self.cursor.bump();
                    indentation = Indentation::root();
                }
                _ => break,
            }
        }

        // Handle indentation if this is a new, not all empty, logical line
        if !matches!(self.cursor.first(), '\n' | '\r' | '#' | EOF_CHAR) {
            self.state = State::NonEmptyLogicalLine;

            // Set to false so that we don't handle indentation on the next call.
            return self.handle_indentation(indentation);
        }

        None
    }

    fn handle_indentation(&mut self, indentation: Indentation) -> Option<TokenKind> {
        match self.indentations.current().try_compare(indentation) {
            // Dedent
            Ok(Ordering::Greater) => {
                self.pending_indentation = Some(indentation);

                if self.indentations.dedent_one(indentation).is_err() {
                    return Some(self.push_error(LexicalError::new(
                        LexicalErrorType::IndentationError,
                        self.token_range(),
                    )));
                }

                // The lexer might've eaten some whitespaces to calculate the `indentation`. For
                // example:
                //
                // ```py
                // if first:
                //     if second:
                //         pass
                //     foo
                // #   ^
                // ```
                //
                // Here, the cursor is at `^` and the `indentation` contains the whitespaces before
                // the `pass` token.
                self.cursor.start_token();

                Some(TokenKind::Dedent)
            }

            Ok(Ordering::Equal) => None,

            // Indent
            Ok(Ordering::Less) => {
                self.indentations.indent(indentation);
                Some(TokenKind::Indent)
            }
            Err(_) => Some(self.push_error(LexicalError::new(
                LexicalErrorType::IndentationError,
                self.token_range(),
            ))),
        }
    }

    fn skip_whitespace(&mut self) -> Result<(), LexicalError> {
        loop {
            match self.cursor.first() {
                ' ' => {
                    self.cursor.bump();
                }
                '\t' => {
                    self.cursor.bump();
                }
                '\\' => {
                    self.cursor.bump();
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else if !self.cursor.eat_char('\n') {
                        return Err(LexicalError::new(
                            LexicalErrorType::LineContinuationError,
                            TextRange::at(self.offset() - '\\'.text_len(), '\\'.text_len()),
                        ));
                    }
                    if self.cursor.is_eof() {
                        return Err(LexicalError::new(LexicalErrorType::Eof, self.token_range()));
                    }
                }
                // Form feed
                '\x0C' => {
                    self.cursor.bump();
                }
                _ => break,
            }
        }

        Ok(())
    }

    // Dispatch based on the given character.
    fn consume_ascii_character(&mut self, c: char) -> TokenKind {
        let token = match c {
            c if is_ascii_identifier_start(c) => self.lex_identifier(c),
            '0'..='9' => self.lex_number(c),
            '#' => return self.lex_comment(),
            '\'' | '"' => self.lex_string(c),
            '=' => {
                if self.cursor.eat_char('=') {
                    TokenKind::EqEqual
                } else {
                    self.state = State::AfterEqual;
                    return TokenKind::Equal;
                }
            }
            '+' => {
                if self.cursor.eat_char('=') {
                    TokenKind::PlusEqual
                } else {
                    TokenKind::Plus
                }
            }
            '*' => {
                if self.cursor.eat_char('=') {
                    TokenKind::StarEqual
                } else if self.cursor.eat_char('*') {
                    if self.cursor.eat_char('=') {
                        TokenKind::DoubleStarEqual
                    } else {
                        TokenKind::DoubleStar
                    }
                } else {
                    TokenKind::Star
                }
            }

            c @ ('%' | '!')
                if self.mode == Mode::Ipython
                    && self.state.is_after_equal()
                    && self.nesting == 0 =>
            {
                // SAFETY: Safe because `c` has been matched against one of the possible escape command token
                self.lex_ipython_escape_command(IpyEscapeKind::try_from(c).unwrap())
            }

            c @ ('%' | '!' | '?' | '/' | ';' | ',')
                if self.mode == Mode::Ipython && self.state.is_new_logical_line() =>
            {
                let kind = if let Ok(kind) = IpyEscapeKind::try_from([c, self.cursor.first()]) {
                    self.cursor.bump();
                    kind
                } else {
                    // SAFETY: Safe because `c` has been matched against one of the possible escape command token
                    IpyEscapeKind::try_from(c).unwrap()
                };

                self.lex_ipython_escape_command(kind)
            }

            '?' if self.mode == Mode::Ipython => TokenKind::Question,

            '/' => {
                if self.cursor.eat_char('=') {
                    TokenKind::SlashEqual
                } else if self.cursor.eat_char('/') {
                    if self.cursor.eat_char('=') {
                        TokenKind::DoubleSlashEqual
                    } else {
                        TokenKind::DoubleSlash
                    }
                } else {
                    TokenKind::Slash
                }
            }
            '%' => {
                if self.cursor.eat_char('=') {
                    TokenKind::PercentEqual
                } else {
                    TokenKind::Percent
                }
            }
            '|' => {
                if self.cursor.eat_char('=') {
                    TokenKind::VbarEqual
                } else {
                    TokenKind::Vbar
                }
            }
            '^' => {
                if self.cursor.eat_char('=') {
                    TokenKind::CircumflexEqual
                } else {
                    TokenKind::CircumFlex
                }
            }
            '&' => {
                if self.cursor.eat_char('=') {
                    TokenKind::AmperEqual
                } else {
                    TokenKind::Amper
                }
            }
            '-' => {
                if self.cursor.eat_char('=') {
                    TokenKind::MinusEqual
                } else if self.cursor.eat_char('>') {
                    TokenKind::Rarrow
                } else {
                    TokenKind::Minus
                }
            }
            '@' => {
                if self.cursor.eat_char('=') {
                    TokenKind::AtEqual
                } else {
                    TokenKind::At
                }
            }
            '!' => {
                if self.cursor.eat_char('=') {
                    TokenKind::NotEqual
                } else {
                    TokenKind::Exclamation
                }
            }
            '~' => TokenKind::Tilde,
            '(' => {
                self.nesting += 1;
                TokenKind::Lpar
            }
            ')' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::Rpar
            }
            '[' => {
                self.nesting += 1;
                TokenKind::Lsqb
            }
            ']' => {
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::Rsqb
            }
            '{' => {
                self.nesting += 1;
                TokenKind::Lbrace
            }
            '}' => {
                if let Some(interpolated_string) = self.interpolated_strings.current_mut() {
                    if interpolated_string.nesting() == self.nesting {
                        let error_type = LexicalErrorType::from_interpolated_string_error(
                            InterpolatedStringErrorType::SingleRbrace,
                            interpolated_string.kind(),
                        );
                        return self.push_error(LexicalError::new(error_type, self.token_range()));
                    }
                    interpolated_string.try_end_format_spec(self.nesting);
                }
                self.nesting = self.nesting.saturating_sub(1);
                TokenKind::Rbrace
            }
            ':' => {
                if self
                    .interpolated_strings
                    .current_mut()
                    .is_some_and(|interpolated_string| {
                        interpolated_string.try_start_format_spec(self.nesting)
                    })
                {
                    TokenKind::Colon
                } else if self.cursor.eat_char('=') {
                    TokenKind::ColonEqual
                } else {
                    TokenKind::Colon
                }
            }
            ';' => TokenKind::Semi,
            '<' => {
                if self.cursor.eat_char('<') {
                    if self.cursor.eat_char('=') {
                        TokenKind::LeftShiftEqual
                    } else {
                        TokenKind::LeftShift
                    }
                } else if self.cursor.eat_char('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }
            '>' => {
                if self.cursor.eat_char('>') {
                    if self.cursor.eat_char('=') {
                        TokenKind::RightShiftEqual
                    } else {
                        TokenKind::RightShift
                    }
                } else if self.cursor.eat_char('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }
            ',' => TokenKind::Comma,
            '.' => {
                if self.cursor.first().is_ascii_digit() {
                    self.lex_decimal_number('.')
                } else if self.cursor.eat_char2('.', '.') {
                    TokenKind::Ellipsis
                } else {
                    TokenKind::Dot
                }
            }
            '\n' => {
                return if self.nesting == 0 && !self.state.is_new_logical_line() {
                    self.state = State::AfterNewline;
                    TokenKind::Newline
                } else {
                    if let Some(interpolated_string) = self.interpolated_strings.current_mut() {
                        interpolated_string.try_end_format_spec(self.nesting);
                    }
                    TokenKind::NonLogicalNewline
                };
            }
            '\r' => {
                self.cursor.eat_char('\n');

                return if self.nesting == 0 && !self.state.is_new_logical_line() {
                    self.state = State::AfterNewline;
                    TokenKind::Newline
                } else {
                    if let Some(interpolated_string) = self.interpolated_strings.current_mut() {
                        interpolated_string.try_end_format_spec(self.nesting);
                    }
                    TokenKind::NonLogicalNewline
                };
            }

            _ => {
                self.state = State::Other;

                return self.push_error(LexicalError::new(
                    LexicalErrorType::UnrecognizedToken { tok: c },
                    self.token_range(),
                ));
            }
        };

        self.state = State::Other;

        token
    }

    /// Lex an identifier. Also used for keywords and string/bytes literals with a prefix.
    fn lex_identifier(&mut self, first: char) -> TokenKind {
        // Detect potential string like rb'' b'' f'' t'' u'' r''
        let quote = match (first, self.cursor.first()) {
            (_, quote @ ('\'' | '"')) => self.try_single_char_prefix(first).then(|| {
                self.cursor.bump();
                quote
            }),
            (_, second) if is_quote(self.cursor.second()) => {
                self.try_double_char_prefix([first, second]).then(|| {
                    self.cursor.bump();
                    // SAFETY: Safe because of the `is_quote` check in this match arm's guard
                    self.cursor.bump().unwrap()
                })
            }
            _ => None,
        };

        if let Some(quote) = quote {
            if self.current_flags.is_interpolated_string() {
                if let Some(kind) = self.lex_interpolated_string_start(quote) {
                    return kind;
                }
            }

            return self.lex_string(quote);
        }

        // Keep track of whether the identifier is ASCII-only or not.
        //
        // This is important because Python applies NFKC normalization to
        // identifiers: https://docs.python.org/3/reference/lexical_analysis.html#identifiers.
        // We need to therefore do the same in our lexer, but applying NFKC normalization
        // unconditionally is extremely expensive. If we know an identifier is ASCII-only,
        // (by far the most common case), we can skip NFKC normalization of the identifier.
        let mut is_ascii = first.is_ascii();
        self.cursor
            .eat_while(|c| is_identifier_continuation(c, &mut is_ascii));

        let text = self.token_text();

        if !is_ascii {
            self.current_value = TokenValue::Name(text.nfkc().collect::<Name>());
            return TokenKind::Name;
        }

        // Short circuit for names that are longer than any known keyword.
        // It helps Rust to predict that the Name::new call in the keyword match's default branch
        // is guaranteed to fit into a stack allocated (inline) Name.
        if text.len() > 8 {
            self.current_value = TokenValue::Name(Name::new(text));
            return TokenKind::Name;
        }

        match text {
            "False" => TokenKind::False,
            "None" => TokenKind::None,
            "True" => TokenKind::True,
            "and" => TokenKind::And,
            "as" => TokenKind::As,
            "assert" => TokenKind::Assert,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "break" => TokenKind::Break,
            "case" => TokenKind::Case,
            "class" => TokenKind::Class,
            "continue" => TokenKind::Continue,
            "def" => TokenKind::Def,
            "del" => TokenKind::Del,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "except" => TokenKind::Except,
            "finally" => TokenKind::Finally,
            "for" => TokenKind::For,
            "from" => TokenKind::From,
            "global" => TokenKind::Global,
            "if" => TokenKind::If,
            "import" => TokenKind::Import,
            "in" => TokenKind::In,
            "is" => TokenKind::Is,
            "lambda" => TokenKind::Lambda,
            "match" => TokenKind::Match,
            "nonlocal" => TokenKind::Nonlocal,
            "not" => TokenKind::Not,
            "or" => TokenKind::Or,
            "pass" => TokenKind::Pass,
            "raise" => TokenKind::Raise,
            "return" => TokenKind::Return,
            "try" => TokenKind::Try,
            "type" => TokenKind::Type,
            "while" => TokenKind::While,
            "with" => TokenKind::With,
            "yield" => TokenKind::Yield,
            _ => {
                self.current_value = TokenValue::Name(Name::new(text));
                TokenKind::Name
            }
        }
    }

    /// Try lexing the single character string prefix, updating the token flags accordingly.
    /// Returns `true` if it matches.
    fn try_single_char_prefix(&mut self, first: char) -> bool {
        match first {
            'f' | 'F' => self.current_flags |= TokenFlags::F_STRING,
            't' | 'T' => self.current_flags |= TokenFlags::T_STRING,
            'u' | 'U' => self.current_flags |= TokenFlags::UNICODE_STRING,
            'b' | 'B' => self.current_flags |= TokenFlags::BYTE_STRING,
            'r' => self.current_flags |= TokenFlags::RAW_STRING_LOWERCASE,
            'R' => self.current_flags |= TokenFlags::RAW_STRING_UPPERCASE,
            _ => return false,
        }
        true
    }

    /// Try lexing the double character string prefix, updating the token flags accordingly.
    /// Returns `true` if it matches.
    fn try_double_char_prefix(&mut self, value: [char; 2]) -> bool {
        match value {
            ['r', 'f' | 'F'] | ['f' | 'F', 'r'] => {
                self.current_flags |= TokenFlags::F_STRING | TokenFlags::RAW_STRING_LOWERCASE;
            }
            ['R', 'f' | 'F'] | ['f' | 'F', 'R'] => {
                self.current_flags |= TokenFlags::F_STRING | TokenFlags::RAW_STRING_UPPERCASE;
            }
            ['r', 't' | 'T'] | ['t' | 'T', 'r'] => {
                self.current_flags |= TokenFlags::T_STRING | TokenFlags::RAW_STRING_LOWERCASE;
            }
            ['R', 't' | 'T'] | ['t' | 'T', 'R'] => {
                self.current_flags |= TokenFlags::T_STRING | TokenFlags::RAW_STRING_UPPERCASE;
            }
            ['r', 'b' | 'B'] | ['b' | 'B', 'r'] => {
                self.current_flags |= TokenFlags::BYTE_STRING | TokenFlags::RAW_STRING_LOWERCASE;
            }
            ['R', 'b' | 'B'] | ['b' | 'B', 'R'] => {
                self.current_flags |= TokenFlags::BYTE_STRING | TokenFlags::RAW_STRING_UPPERCASE;
            }
            _ => return false,
        }
        true
    }

    /// Lex a f-string or t-string start token if positioned at the start of an f-string or t-string.
    fn lex_interpolated_string_start(&mut self, quote: char) -> Option<TokenKind> {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), quote);

        if quote == '"' {
            self.current_flags |= TokenFlags::DOUBLE_QUOTES;
        }

        if self.cursor.eat_char2(quote, quote) {
            self.current_flags |= TokenFlags::TRIPLE_QUOTED_STRING;
        }

        let ftcontext = InterpolatedStringContext::new(self.current_flags, self.nesting)?;

        let kind = ftcontext.kind();

        self.interpolated_strings.push(ftcontext);

        Some(kind.start_token())
    }

    /// Lex an f-string or t-string middle or end token.
    fn lex_interpolated_string_middle_or_end(&mut self) -> Option<TokenKind> {
        // SAFETY: Safe because the function is only called when `self.fstrings` is not empty.
        let interpolated_string = self.interpolated_strings.current().unwrap();
        let string_kind = interpolated_string.kind();
        let interpolated_flags = interpolated_string.flags();

        // Check if we're at the end of the f-string.
        if interpolated_string.is_triple_quoted() {
            let quote_char = interpolated_string.quote_char();
            if self.cursor.eat_char3(quote_char, quote_char, quote_char) {
                self.current_flags = interpolated_string.flags();
                return Some(string_kind.end_token());
            }
        } else if self.cursor.eat_char(interpolated_string.quote_char()) {
            self.current_flags = interpolated_string.flags();
            return Some(string_kind.end_token());
        }

        // We have to decode `{{` and `}}` into `{` and `}` respectively. As an
        // optimization, we only allocate a new string we find any escaped curly braces,
        // otherwise this string will remain empty and we'll use a source slice instead.
        let mut normalized = String::new();

        // Tracks the last offset of token value that has been written to `normalized`.
        let mut last_offset = self.offset();

        // This isn't going to change for the duration of the loop.
        let in_format_spec = interpolated_string.is_in_format_spec(self.nesting);

        let mut in_named_unicode = false;

        loop {
            match self.cursor.first() {
                // The condition is to differentiate between the `NUL` (`\0`) character
                // in the source code and the one returned by `self.cursor.first()` when
                // we reach the end of the source code.
                EOF_CHAR if self.cursor.is_eof() => {
                    let error = if interpolated_string.is_triple_quoted() {
                        InterpolatedStringErrorType::UnterminatedTripleQuotedString
                    } else {
                        InterpolatedStringErrorType::UnterminatedString
                    };

                    self.nesting = interpolated_string.nesting();
                    self.interpolated_strings.pop();
                    self.current_flags |= TokenFlags::UNCLOSED_STRING;
                    self.push_error(LexicalError::new(
                        LexicalErrorType::from_interpolated_string_error(error, string_kind),
                        self.token_range(),
                    ));

                    break;
                }
                '\n' | '\r' if !interpolated_string.is_triple_quoted() => {
                    // https://github.com/astral-sh/ruff/issues/18632

                    let error_type = if in_format_spec {
                        InterpolatedStringErrorType::NewlineInFormatSpec
                    } else {
                        InterpolatedStringErrorType::UnterminatedString
                    };

                    self.nesting = interpolated_string.nesting();
                    self.interpolated_strings.pop();
                    self.current_flags |= TokenFlags::UNCLOSED_STRING;

                    self.push_error(LexicalError::new(
                        LexicalErrorType::from_interpolated_string_error(error_type, string_kind),
                        self.token_range(),
                    ));

                    break;
                }
                '\\' => {
                    self.cursor.bump(); // '\'
                    if matches!(self.cursor.first(), '{' | '}') {
                        // Don't consume `{` or `}` as we want them to be emitted as tokens.
                        // They will be handled in the next iteration.
                        continue;
                    } else if !interpolated_string.is_raw_string() {
                        if self.cursor.eat_char2('N', '{') {
                            in_named_unicode = true;
                            continue;
                        }
                    }
                    // Consume the escaped character.
                    if self.cursor.eat_char('\r') {
                        self.cursor.eat_char('\n');
                    } else {
                        self.cursor.bump();
                    }
                }
                quote @ ('\'' | '"') if quote == interpolated_string.quote_char() => {
                    if let Some(triple_quotes) = interpolated_string.triple_quotes() {
                        if self.cursor.rest().starts_with(triple_quotes) {
                            break;
                        }
                        self.cursor.bump();
                    } else {
                        break;
                    }
                }
                '{' => {
                    if self.cursor.second() == '{' && !in_format_spec {
                        self.cursor.bump();
                        normalized
                            .push_str(&self.source[TextRange::new(last_offset, self.offset())]);
                        self.cursor.bump(); // Skip the second `{`
                        last_offset = self.offset();
                    } else {
                        break;
                    }
                }
                '}' => {
                    if in_named_unicode {
                        in_named_unicode = false;
                        self.cursor.bump();
                    } else if self.cursor.second() == '}' && !in_format_spec {
                        self.cursor.bump();
                        normalized
                            .push_str(&self.source[TextRange::new(last_offset, self.offset())]);
                        self.cursor.bump(); // Skip the second `}`
                        last_offset = self.offset();
                    } else {
                        break;
                    }
                }
                _ => {
                    self.cursor.bump();
                }
            }
        }
        let range = self.token_range();
        if range.is_empty() {
            return None;
        }

        let value = if normalized.is_empty() {
            self.source[range].to_string()
        } else {
            normalized.push_str(&self.source[TextRange::new(last_offset, self.offset())]);
            normalized
        };

        self.current_value = TokenValue::InterpolatedStringMiddle(value.into_boxed_str());

        self.current_flags = interpolated_flags;
        Some(string_kind.middle_token())
    }

    /// Lex a string literal.
    fn lex_string(&mut self, quote: char) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), quote);

        if quote == '"' {
            self.current_flags |= TokenFlags::DOUBLE_QUOTES;
        }

        // If the next two characters are also the quote character, then we have a triple-quoted
        // string; consume those two characters and ensure that we require a triple-quote to close
        if self.cursor.eat_char2(quote, quote) {
            self.current_flags |= TokenFlags::TRIPLE_QUOTED_STRING;
        }

        let value_start = self.offset();

        let quote_byte = u8::try_from(quote).expect("char that fits in u8");
        let value_end = if self.current_flags.is_triple_quoted() {
            // For triple-quoted strings, scan until we find the closing quote (ignoring escaped
            // quotes) or the end of the file.
            loop {
                let Some(index) = memchr::memchr(quote_byte, self.cursor.rest().as_bytes()) else {
                    self.cursor.skip_to_end();

                    self.current_flags |= TokenFlags::UNCLOSED_STRING;
                    self.push_error(LexicalError::new(
                        LexicalErrorType::UnclosedStringError,
                        self.token_range(),
                    ));
                    break self.offset();
                };

                // Rare case: if there are an odd number of backslashes before the quote, then
                // the quote is escaped and we should continue scanning.
                let num_backslashes = self.cursor.rest().as_bytes()[..index]
                    .iter()
                    .rev()
                    .take_while(|&&c| c == b'\\')
                    .count();

                // Advance the cursor past the quote and continue scanning.
                self.cursor.skip_bytes(index + 1);

                // If the character is escaped, continue scanning.
                if num_backslashes % 2 == 1 {
                    continue;
                }

                // Otherwise, if it's followed by two more quotes, then we're done.
                if self.cursor.eat_char2(quote, quote) {
                    break self.offset() - TextSize::new(3);
                }
            }
        } else {
            // For non-triple-quoted strings, scan until we find the closing quote, but end early
            // if we encounter a newline or the end of the file.
            loop {
                let Some(index) =
                    memchr::memchr3(quote_byte, b'\r', b'\n', self.cursor.rest().as_bytes())
                else {
                    self.cursor.skip_to_end();
                    self.current_flags |= TokenFlags::UNCLOSED_STRING;

                    self.push_error(LexicalError::new(
                        LexicalErrorType::UnclosedStringError,
                        self.token_range(),
                    ));

                    break self.offset();
                };

                // Rare case: if there are an odd number of backslashes before the quote, then
                // the quote is escaped and we should continue scanning.
                let num_backslashes = self.cursor.rest().as_bytes()[..index]
                    .iter()
                    .rev()
                    .take_while(|&&c| c == b'\\')
                    .count();

                // Skip up to the current character.
                self.cursor.skip_bytes(index);

                // Lookahead because we want to bump only if it's a quote or being escaped.
                let quote_or_newline = self.cursor.first();

                // If the character is escaped, continue scanning.
                if num_backslashes % 2 == 1 {
                    self.cursor.bump();
                    if quote_or_newline == '\r' {
                        self.cursor.eat_char('\n');
                    }
                    continue;
                }

                match quote_or_newline {
                    '\r' | '\n' => {
                        self.current_flags |= TokenFlags::UNCLOSED_STRING;
                        self.push_error(LexicalError::new(
                            LexicalErrorType::UnclosedStringError,
                            self.token_range(),
                        ));
                        break self.offset();
                    }
                    ch if ch == quote => {
                        let value_end = self.offset();
                        self.cursor.bump();
                        break value_end;
                    }
                    _ => unreachable!("memchr2 returned an index that is not a quote or a newline"),
                }
            }
        };

        self.current_value = TokenValue::String(
            self.source[TextRange::new(value_start, value_end)]
                .to_string()
                .into_boxed_str(),
        );

        TokenKind::String
    }

    /// Numeric lexing. The feast can start!
    fn lex_number(&mut self, first: char) -> TokenKind {
        if first == '0' {
            if self.cursor.eat_if(|c| matches!(c, 'x' | 'X')).is_some() {
                self.lex_number_radix(Radix::Hex)
            } else if self.cursor.eat_if(|c| matches!(c, 'o' | 'O')).is_some() {
                self.lex_number_radix(Radix::Octal)
            } else if self.cursor.eat_if(|c| matches!(c, 'b' | 'B')).is_some() {
                self.lex_number_radix(Radix::Binary)
            } else {
                self.lex_decimal_number(first)
            }
        } else {
            self.lex_decimal_number(first)
        }
    }

    /// Lex a hex/octal/decimal/binary number without a decimal point.
    fn lex_number_radix(&mut self, radix: Radix) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert!(matches!(
            self.cursor.previous().to_ascii_lowercase(),
            'x' | 'o' | 'b'
        ));

        // Lex the portion of the token after the base prefix (e.g., `9D5` in `0x9D5`).
        let mut number = LexedText::new(self.offset(), self.source);
        self.radix_run(&mut number, radix);

        // Extract the entire number, including the base prefix (e.g., `0x9D5`).
        let token = &self.source[self.token_range()];

        let value = match Int::from_str_radix(number.as_str(), radix.as_u32(), token) {
            Ok(int) => int,
            Err(err) => {
                return self.push_error(LexicalError::new(
                    LexicalErrorType::OtherError(format!("{err:?}").into_boxed_str()),
                    self.token_range(),
                ));
            }
        };
        self.current_value = TokenValue::Int(value);
        TokenKind::Int
    }

    /// Lex a normal number, that is, no octal, hex or binary number.
    fn lex_decimal_number(&mut self, first_digit_or_dot: char) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert!(self.cursor.previous().is_ascii_digit() || self.cursor.previous() == '.');
        let start_is_zero = first_digit_or_dot == '0';

        let mut number = LexedText::new(self.token_start(), self.source);
        if first_digit_or_dot != '.' {
            number.push(first_digit_or_dot);
            self.radix_run(&mut number, Radix::Decimal);
        }

        let is_float = if first_digit_or_dot == '.' || self.cursor.eat_char('.') {
            number.push('.');

            if self.cursor.eat_char('_') {
                return self.push_error(LexicalError::new(
                    LexicalErrorType::OtherError("Invalid Syntax".to_string().into_boxed_str()),
                    TextRange::new(self.offset() - TextSize::new(1), self.offset()),
                ));
            }

            self.radix_run(&mut number, Radix::Decimal);
            true
        } else {
            // Normal number:
            false
        };

        let is_float = match self.cursor.rest().as_bytes() {
            [b'e' | b'E', b'0'..=b'9', ..] | [b'e' | b'E', b'-' | b'+', b'0'..=b'9', ..] => {
                // 'e' | 'E'
                number.push(self.cursor.bump().unwrap());

                if let Some(sign) = self.cursor.eat_if(|c| matches!(c, '+' | '-')) {
                    number.push(sign);
                }

                self.radix_run(&mut number, Radix::Decimal);

                true
            }
            _ => is_float,
        };

        if is_float {
            // Improvement: Use `Cow` instead of pushing to value text
            let Ok(value) = f64::from_str(number.as_str()) else {
                return self.push_error(LexicalError::new(
                    LexicalErrorType::OtherError(
                        "Invalid decimal literal".to_string().into_boxed_str(),
                    ),
                    self.token_range(),
                ));
            };

            // Parse trailing 'j':
            if self.cursor.eat_if(|c| matches!(c, 'j' | 'J')).is_some() {
                self.current_value = TokenValue::Complex {
                    real: 0.0,
                    imag: value,
                };
                TokenKind::Complex
            } else {
                self.current_value = TokenValue::Float(value);
                TokenKind::Float
            }
        } else {
            // Parse trailing 'j':
            if self.cursor.eat_if(|c| matches!(c, 'j' | 'J')).is_some() {
                let imag = f64::from_str(number.as_str()).unwrap();
                self.current_value = TokenValue::Complex { real: 0.0, imag };
                TokenKind::Complex
            } else {
                let value = match Int::from_str(number.as_str()) {
                    Ok(value) => {
                        if start_is_zero && value.as_u8() != Some(0) {
                            // Leading zeros in decimal integer literals are not permitted.
                            return self.push_error(LexicalError::new(
                                LexicalErrorType::OtherError(
                                    "Invalid decimal integer literal"
                                        .to_string()
                                        .into_boxed_str(),
                                ),
                                self.token_range(),
                            ));
                        }
                        value
                    }
                    Err(err) => {
                        return self.push_error(LexicalError::new(
                            LexicalErrorType::OtherError(format!("{err:?}").into_boxed_str()),
                            self.token_range(),
                        ));
                    }
                };
                self.current_value = TokenValue::Int(value);
                TokenKind::Int
            }
        }
    }

    /// Consume a sequence of numbers with the given radix,
    /// the digits can be decorated with underscores
    /// like this: '`1_2_3_4`' == '1234'
    fn radix_run(&mut self, number: &mut LexedText, radix: Radix) {
        loop {
            if let Some(c) = self.cursor.eat_if(|c| radix.is_digit(c)) {
                number.push(c);
            }
            // Number that contains `_` separators. Remove them from the parsed text.
            else if self.cursor.first() == '_' && radix.is_digit(self.cursor.second()) {
                // Skip over `_`
                self.cursor.bump();
                number.skip_char();
            } else {
                break;
            }
        }
    }

    /// Lex a single comment.
    fn lex_comment(&mut self) -> TokenKind {
        #[cfg(debug_assertions)]
        debug_assert_eq!(self.cursor.previous(), '#');

        let bytes = self.cursor.rest().as_bytes();
        let offset = memchr::memchr2(b'\n', b'\r', bytes).unwrap_or(bytes.len());
        self.cursor.skip_bytes(offset);

        TokenKind::Comment
    }

    /// Lex a single IPython escape command.
    fn lex_ipython_escape_command(&mut self, escape_kind: IpyEscapeKind) -> TokenKind {
        let mut value = String::new();

        loop {
            match self.cursor.first() {
                '\\' => {
                    // Only skip the line continuation if it is followed by a newline
                    // otherwise it is a normal backslash which is part of the magic command:
                    //
                    //        Skip this backslash
                    //        v
                    //   !pwd \
                    //      && ls -a | sed 's/^/\\    /'
                    //                          ^^
                    //                          Don't skip these backslashes
                    if self.cursor.second() == '\r' {
                        self.cursor.bump();
                        self.cursor.bump();
                        self.cursor.eat_char('\n');
                        continue;
                    } else if self.cursor.second() == '\n' {
                        self.cursor.bump();
                        self.cursor.bump();
                        continue;
                    }

                    self.cursor.bump();
                    value.push('\\');
                }
                // Help end escape commands are those that end with 1 or 2 question marks.
                // Here, we're only looking for a subset of help end escape commands which
                // are the ones that has the escape token at the start of the line as well.
                // On the other hand, we're not looking for help end escape commands that
                // are strict in the sense that the escape token is only at the end. For example,
                //
                //   * `%foo?` is recognized as a help end escape command but not as a strict one.
                //   * `foo?` is recognized as a strict help end escape command which is not
                //     lexed here but is identified at the parser level.
                //
                // Help end escape commands implemented in the IPython codebase using regex:
                // https://github.com/ipython/ipython/blob/292e3a23459ca965b8c1bfe2c3707044c510209a/IPython/core/inputtransformer2.py#L454-L462
                '?' => {
                    self.cursor.bump();
                    let mut question_count = 1u32;
                    while self.cursor.eat_char('?') {
                        question_count += 1;
                    }

                    // The original implementation in the IPython codebase is based on regex which
                    // means that it's strict in the sense that it won't recognize a help end escape:
                    //   * If there's any whitespace before the escape token (e.g. `%foo ?`)
                    //   * If there are more than 2 question mark tokens (e.g. `%foo???`)
                    // which is what we're doing here as well. In that case, we'll continue with
                    // the prefixed escape token.
                    //
                    // Now, the whitespace and empty value check also makes sure that an empty
                    // command (e.g. `%?` or `? ??`, no value after/between the escape tokens)
                    // is not recognized as a help end escape command. So, `%?` and `? ??` are
                    // `IpyEscapeKind::Magic` and `IpyEscapeKind::Help` because of the initial `%` and `??`
                    // tokens.
                    if question_count > 2
                        || value.chars().last().is_none_or(is_python_whitespace)
                        || !matches!(self.cursor.first(), '\n' | '\r' | EOF_CHAR)
                    {
                        // Not a help end escape command, so continue with the lexing.
                        value.reserve(question_count as usize);
                        for _ in 0..question_count {
                            value.push('?');
                        }
                        continue;
                    }

                    if escape_kind.is_help() {
                        // If we've recognize this as a help end escape command, then
                        // any question mark token / whitespaces at the start are not
                        // considered as part of the value.
                        //
                        // For example, `??foo?` is recognized as `IpyEscapeKind::Help` and
                        // `value` is `foo` instead of `??foo`.
                        value = value.trim_start_matches([' ', '?']).to_string();
                    } else if escape_kind.is_magic() {
                        // Between `%` and `?` (at the end), the `?` takes priority
                        // over the `%` so `%foo?` is recognized as `IpyEscapeKind::Help`
                        // and `value` is `%foo` instead of `foo`. So, we need to
                        // insert the magic escape token at the start.
                        value.insert_str(0, escape_kind.as_str());
                    }

                    let kind = match question_count {
                        1 => IpyEscapeKind::Help,
                        2 => IpyEscapeKind::Help2,
                        _ => unreachable!("`question_count` is always 1 or 2"),
                    };

                    self.current_value = TokenValue::IpyEscapeCommand {
                        kind,
                        value: value.into_boxed_str(),
                    };

                    return TokenKind::IpyEscapeCommand;
                }
                '\n' | '\r' | EOF_CHAR => {
                    self.current_value = TokenValue::IpyEscapeCommand {
                        kind: escape_kind,
                        value: value.into_boxed_str(),
                    };

                    return TokenKind::IpyEscapeCommand;
                }
                c => {
                    self.cursor.bump();
                    value.push(c);
                }
            }
        }
    }

    fn consume_end(&mut self) -> TokenKind {
        // We reached end of file.

        // First, finish any unterminated interpolated-strings.
        while let Some(interpolated_string) = self.interpolated_strings.pop() {
            self.nesting = interpolated_string.nesting();
            self.push_error(LexicalError::new(
                LexicalErrorType::from_interpolated_string_error(
                    InterpolatedStringErrorType::UnterminatedString,
                    interpolated_string.kind(),
                ),
                self.token_range(),
            ));
        }

        // Second, finish all nestings.
        // For Mode::ParenthesizedExpression we start with nesting level 1.
        // So we check if we end with that level.
        let init_nesting = u32::from(self.mode == Mode::ParenthesizedExpression);

        if self.nesting > init_nesting {
            // Reset the nesting to avoid going into infinite loop.
            self.nesting = 0;
            return self.push_error(LexicalError::new(LexicalErrorType::Eof, self.token_range()));
        }

        // Next, insert a trailing newline, if required.
        if !self.state.is_new_logical_line() {
            self.state = State::AfterNewline;
            TokenKind::Newline
        }
        // Next, flush the indentation stack to zero.
        else if self.indentations.dedent().is_some() {
            TokenKind::Dedent
        } else {
            TokenKind::EndOfFile
        }
    }

    /// Re-lex the [`NonLogicalNewline`] token at the given position in the context of a logical
    /// line.
    ///
    /// Returns a boolean indicating whether the lexer's position has changed. This could result
    /// into the new current token being different than the previous current token but is not
    /// necessarily true. If the return value is `true` then the caller is responsible for updating
    /// it's state accordingly.
    ///
    /// This method is a no-op if the lexer isn't in a parenthesized context.
    ///
    /// ## Explanation
    ///
    /// The lexer emits two different kinds of newline token based on the context. If it's in a
    /// parenthesized context, it'll emit a [`NonLogicalNewline`] token otherwise it'll emit a
    /// regular [`Newline`] token. Based on the type of newline token, the lexer will consume and
    /// emit the indentation tokens appropriately which affects the structure of the code.
    ///
    /// For example:
    /// ```py
    /// if call(foo
    ///     def bar():
    ///         pass
    /// ```
    ///
    /// Here, the lexer emits a [`NonLogicalNewline`] token after `foo` which means that the lexer
    /// doesn't emit an `Indent` token before the `def` keyword. This leads to an AST which
    /// considers the function `bar` as part of the module block and the `if` block remains empty.
    ///
    /// This method is to facilitate the parser if it recovers from these kind of scenarios so that
    /// the lexer can then re-lex a [`NonLogicalNewline`] token to a [`Newline`] token which in
    /// turn helps the parser to build the correct AST.
    ///
    /// In the above snippet, it would mean that this method would move the lexer back to the
    /// newline character after the `foo` token and emit it as a [`Newline`] token instead of
    /// [`NonLogicalNewline`]. This means that the next token emitted by the lexer would be an
    /// `Indent` token.
    ///
    /// There are cases where the lexer's position will change but the re-lexed token will remain
    /// the same. This is to help the parser to add the error message at an appropriate location.
    /// Consider the following example:
    ///
    /// ```py
    /// if call(foo, [a, b
    ///     def bar():
    ///         pass
    /// ```
    ///
    /// Here, the parser recovers from two unclosed parenthesis. The inner unclosed `[` will call
    /// into the re-lexing logic and reduce the nesting level from 2 to 1. And, the re-lexing logic
    /// will move the lexer at the newline after `b` but still emit a [`NonLogicalNewline`] token.
    /// Only after the parser recovers from the outer unclosed `(` does the re-lexing logic emit
    /// the [`Newline`] token.
    ///
    /// [`Newline`]: TokenKind::Newline
    /// [`NonLogicalNewline`]: TokenKind::NonLogicalNewline
    pub(crate) fn re_lex_logical_token(
        &mut self,
        non_logical_newline_start: Option<TextSize>,
    ) -> bool {
        if self.nesting == 0 {
            return false;
        }

        // Reduce the nesting level because the parser recovered from an error inside list parsing
        // i.e., it recovered from an unclosed parenthesis (`(`, `[`, or `{`).
        self.nesting -= 1;

        // The lexer can't be moved back for a triple-quoted f/t-string because the newlines are
        // part of the f/t-string itself, so there is no newline token to be emitted.
        if self.current_flags.is_triple_quoted_interpolated_string() {
            return false;
        }

        let Some(new_position) = non_logical_newline_start else {
            return false;
        };

        // Earlier we reduced the nesting level unconditionally. Now that we know the lexer's
        // position is going to be moved back, the lexer needs to be put back into a
        // parenthesized context if the current token is a closing parenthesis.
        //
        // ```py
        // (a, [b,
        //     c
        // )
        // ```
        //
        // Here, the parser would request to re-lex the token when it's at `)` and can recover
        // from an unclosed `[`. This method will move the lexer back to the newline character
        // after `c` which means it goes back into parenthesized context.
        if matches!(
            self.current_kind,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
        ) {
            self.nesting += 1;
        }

        self.cursor = Cursor::new(self.source);
        self.cursor.skip_bytes(new_position.to_usize());
        self.state = State::Other;
        self.next_token();
        true
    }

    /// Re-lexes an unclosed string token in the context of an interpolated string element.
    ///
    /// ```py
    /// f'{a'
    /// ```
    ///
    /// This method re-lexes the trailing `'` as the end of the f-string rather than the
    /// start of a new string token for better error recovery.
    pub(crate) fn re_lex_string_token_in_interpolation_element(
        &mut self,
        kind: InterpolatedStringKind,
    ) {
        let Some(interpolated_string) = self.interpolated_strings.current() else {
            return;
        };

        let current_string_flags = self.current_flags().as_any_string_flags();

        // Only unclosed strings, that have the same quote character
        if !matches!(self.current_kind, TokenKind::String)
            || !self.current_flags.is_unclosed()
            || current_string_flags.prefix() != AnyStringPrefix::Regular(StringLiteralPrefix::Empty)
            || current_string_flags.quote_style().as_char() != interpolated_string.quote_char()
            || current_string_flags.is_triple_quoted() != interpolated_string.is_triple_quoted()
        {
            return;
        }

        // Only if the string's first line only contains whitespace,
        // or ends in a comment (not `f"{"abc`)
        let first_line = &self.source
            [(self.current_range.start() + current_string_flags.quote_len()).to_usize()..];

        for c in first_line.chars() {
            if matches!(c, '\n' | '\r' | '#') {
                break;
            }

            // `f'{'ab`, we want to parse `ab` as a normal string and not the closing element of the f-string
            if !is_python_whitespace(c) {
                return;
            }
        }

        if self.errors.last().is_some_and(|error| {
            error.location() == self.current_range
                && matches!(error.error(), LexicalErrorType::UnclosedStringError)
        }) {
            self.errors.pop();
        }

        self.current_range =
            TextRange::at(self.current_range.start(), self.current_flags.quote_len());
        self.current_kind = kind.end_token();
        self.current_value = TokenValue::None;
        self.current_flags = TokenFlags::empty();

        self.nesting = interpolated_string.nesting();
        self.interpolated_strings.pop();

        self.cursor = Cursor::new(self.source);
        self.cursor.skip_bytes(self.current_range.end().to_usize());
    }

    /// Re-lex `r"` in a format specifier position.
    ///
    /// `r"` in a format specifier position is unlikely to be the start of a raw string.
    /// Instead, it's the format specifier `!r` followed by the closing quote of the f-string,
    /// when the `}` is missing.
    ///
    /// ```py
    /// f"{test!r"
    /// ```
    ///
    /// This function re-lexes the `r"` as `r` (a name token). The next `next_token` call will
    /// return a unclosed string token for `"`, which [`Self::re_lex_string_token_in_interpolation_element`]
    /// can then re-lex as the end of the f-string.
    pub(crate) fn re_lex_raw_string_in_format_spec(&mut self) {
        // Re-lex `r"` as `NAME r` followed by an unclosed string
        // `f"{test!r"` -> `f"{test!`, `r`, `"`
        if matches!(self.current_kind, TokenKind::String)
            && self.current_flags.is_unclosed()
            && self.current_flags.prefix()
                == AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false })
        {
            if self.errors.last().is_some_and(|error| {
                error.location() == self.current_range
                    && matches!(error.error(), LexicalErrorType::UnclosedStringError)
            }) {
                self.errors.pop();
            }

            self.current_range = TextRange::at(self.current_range.start(), 'r'.text_len());
            self.current_kind = TokenKind::Name;
            self.current_value = TokenValue::Name(Name::new_static("r"));
            self.current_flags = TokenFlags::empty();
            self.cursor = Cursor::new(self.source);
            self.cursor.skip_bytes(self.current_range.end().to_usize());
        }
    }

    #[inline]
    fn token_range(&self) -> TextRange {
        let end = self.offset();
        let len = self.cursor.token_len();

        TextRange::at(end - len, len)
    }

    #[inline]
    fn token_text(&self) -> &'src str {
        &self.source[self.token_range()]
    }

    /// Retrieves the current offset of the cursor within the source code.
    // SAFETY: Lexer doesn't allow files larger than 4GB
    #[expect(clippy::cast_possible_truncation)]
    #[inline]
    fn offset(&self) -> TextSize {
        TextSize::new(self.source.len() as u32) - self.cursor.text_len()
    }

    #[inline]
    fn token_start(&self) -> TextSize {
        self.token_range().start()
    }

    /// Creates a checkpoint to which the lexer can later return to using [`Self::rewind`].
    pub(crate) fn checkpoint(&self) -> LexerCheckpoint {
        LexerCheckpoint {
            value: self.current_value.clone(),
            current_kind: self.current_kind,
            current_range: self.current_range,
            current_flags: self.current_flags,
            cursor_offset: self.offset(),
            state: self.state,
            nesting: self.nesting,
            indentations_checkpoint: self.indentations.checkpoint(),
            pending_indentation: self.pending_indentation,
            interpolated_strings_checkpoint: self.interpolated_strings.checkpoint(),
            errors_position: self.errors.len(),
        }
    }

    /// Restore the lexer to the given checkpoint.
    pub(crate) fn rewind(&mut self, checkpoint: LexerCheckpoint) {
        let LexerCheckpoint {
            value,
            current_kind,
            current_range,
            current_flags,
            cursor_offset,
            state,
            nesting,
            indentations_checkpoint,
            pending_indentation,
            interpolated_strings_checkpoint,
            errors_position,
        } = checkpoint;

        let mut cursor = Cursor::new(self.source);
        // We preserve the previous char using this method.
        cursor.skip_bytes(cursor_offset.to_usize());

        self.current_value = value;
        self.current_kind = current_kind;
        self.current_range = current_range;
        self.current_flags = current_flags;
        self.cursor = cursor;
        self.state = state;
        self.nesting = nesting;
        self.indentations.rewind(indentations_checkpoint);
        self.pending_indentation = pending_indentation;
        self.interpolated_strings
            .rewind(interpolated_strings_checkpoint);
        self.errors.truncate(errors_position);
    }

    pub fn finish(self) -> Vec<LexicalError> {
        self.errors
    }
}

pub(crate) struct LexerCheckpoint {
    value: TokenValue,
    current_kind: TokenKind,
    current_range: TextRange,
    current_flags: TokenFlags,
    cursor_offset: TextSize,
    state: State,
    nesting: u32,
    indentations_checkpoint: IndentationsCheckpoint,
    pending_indentation: Option<Indentation>,
    interpolated_strings_checkpoint: InterpolatedStringsCheckpoint,
    errors_position: usize,
}

#[derive(Copy, Clone, Debug)]
enum State {
    /// Lexer is right at the beginning of the file or after a `Newline` token.
    AfterNewline,

    /// The lexer is at the start of a new logical line but **after** the indentation
    NonEmptyLogicalLine,

    /// Lexer is right after an equal token
    AfterEqual,

    /// Inside of a logical line
    Other,
}

impl State {
    const fn is_after_newline(self) -> bool {
        matches!(self, State::AfterNewline)
    }

    const fn is_new_logical_line(self) -> bool {
        matches!(self, State::AfterNewline | State::NonEmptyLogicalLine)
    }

    const fn is_after_equal(self) -> bool {
        matches!(self, State::AfterEqual)
    }
}

#[derive(Copy, Clone, Debug)]
enum Radix {
    Binary,
    Octal,
    Decimal,
    Hex,
}

impl Radix {
    const fn as_u32(self) -> u32 {
        match self {
            Radix::Binary => 2,
            Radix::Octal => 8,
            Radix::Decimal => 10,
            Radix::Hex => 16,
        }
    }

    const fn is_digit(self, c: char) -> bool {
        match self {
            Radix::Binary => matches!(c, '0'..='1'),
            Radix::Octal => matches!(c, '0'..='7'),
            Radix::Decimal => c.is_ascii_digit(),
            Radix::Hex => c.is_ascii_hexdigit(),
        }
    }
}

const fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '"')
}

const fn is_ascii_identifier_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_')
}

// Checks if the character c is a valid starting character as described
// in https://docs.python.org/3/reference/lexical_analysis.html#identifiers
fn is_unicode_identifier_start(c: char) -> bool {
    is_xid_start(c)
}

/// Checks if the character c is a valid continuation character as described
/// in <https://docs.python.org/3/reference/lexical_analysis.html#identifiers>.
///
/// Additionally, this function also keeps track of whether or not the total
/// identifier is ASCII-only or not by mutably altering a reference to a
/// boolean value passed in.
fn is_identifier_continuation(c: char, identifier_is_ascii_only: &mut bool) -> bool {
    // Arrange things such that ASCII codepoints never
    // result in the slower `is_xid_continue` getting called.
    if c.is_ascii() {
        matches!(c, 'a'..='z' | 'A'..='Z' | '_' | '0'..='9')
    } else {
        *identifier_is_ascii_only = false;
        is_xid_continue(c)
    }
}

enum LexedText<'a> {
    Source { source: &'a str, range: TextRange },
    Owned(String),
}

impl<'a> LexedText<'a> {
    fn new(start: TextSize, source: &'a str) -> Self {
        Self::Source {
            range: TextRange::empty(start),
            source,
        }
    }

    fn push(&mut self, c: char) {
        match self {
            LexedText::Source { range, source } => {
                *range = range.add_end(c.text_len());
                debug_assert!(source[*range].ends_with(c));
            }
            LexedText::Owned(owned) => owned.push(c),
        }
    }

    fn as_str<'b>(&'b self) -> &'b str
    where
        'b: 'a,
    {
        match self {
            LexedText::Source { range, source } => &source[*range],
            LexedText::Owned(owned) => owned,
        }
    }

    fn skip_char(&mut self) {
        match self {
            LexedText::Source { range, source } => {
                *self = LexedText::Owned(source[*range].to_string());
            }
            LexedText::Owned(_) => {}
        }
    }
}

/// Create a new [`Lexer`] for the given source code and [`Mode`].
pub fn lex(source: &str, mode: Mode) -> Lexer<'_> {
    Lexer::new(source, mode, TextSize::default())
}

