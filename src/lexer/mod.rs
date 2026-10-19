//! Qessa Lexer
//!
//! Converts source text into a flat stream of tokens, preserving spans
//! (byte offsets) so that later stages can produce accurate diagnostics.

use std::fmt;

// ---------------------------------------------------------------------------
// Token kinds
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Null,

    // Identifiers & keywords
    Ident(String),
    // Keywords
    KwFn,
    KwLet,
    KwMut,
    KwIf,
    KwElse,
    KwElif,
    KwWhile,
    KwFor,
    KwIn,
    KwReturn,
    KwBreak,
    KwContinue,
    KwStruct,
    KwEnum,
    KwMod,
    KwUse,
    KwMacro,
    KwRegion,
    KwExtern,
    KwMatch,
    KwAs,
    KwTrue,
    KwFalse,
    KwNull,
    KwAnd,
    KwOr,
    KwNot,
    KwTypeInt,
    KwTypeFloat,
    KwTypeStr,
    KwTypeBool,
    KwTypeChar,
    KwTypeVoid,

    // Punctuation
    LParen,      // (
    RParen,      // )
    LBrace,      // {
    RBrace,      // }
    LBracket,    // [
    RBracket,    // ]
    Comma,       // ,
    Dot,         // .
    Colon,       // :
    DoubleColon, // ::
    Semicolon,   // ;
    Arrow,       // ->
    FatArrow,    // =>
    Pipe,        // |
    Pound,       // #
    At,          // @
    Dollar,      // $
    Ampersand,   // &

    // Operators
    Assign,      // =
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    Percent,     // %
    Eq,          // ==
    Neq,         // !=
    Lt,          // <
    Gt,          // >
    Le,          // <=
    Ge,          // >=
    PlusEq,      // +=
    MinusEq,     // -=
    StarEq,      // *=
    SlashEq,     // /=
    Shl,         // <<
    Shr,         // >>
    BitAnd,      // &
    BitOr,       // |
    BitXor,      // ^
    BitNot,      // ~

    // Special
    Newline,
    Eof,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Int(v) => write!(f, "{}", v),
            Tok::Float(v) => write!(f, "{}", v),
            Tok::Str(s) => write!(f, "\"{}\"", s),
            Tok::Char(c) => write!(f, "'{}'", c),
            other => write!(f, "{:?}", other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Spanned {
    pub tok: Tok,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

#[derive(Debug)]
pub struct LexError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lex error at {}:{}: {}", self.line, self.col, self.msg)
    }
}
impl std::error::Error for LexError {}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.src.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn at_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), LexError> {
        // Skip spaces/tabs/CRs (but NOT newlines — those produce Newline tokens).
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') => {
                    self.advance();
                }
                Some(b'/') if self.peek2() == Some(b'/') => {
                    // line comment — skip to end of line (don't consume newline)
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some(b'/') if self.peek2() == Some(b'*') => {
                    // block comment
                    self.advance();
                    self.advance();
                    let mut depth = 1;
                    while depth > 0 && !self.at_eof() {
                        if self.peek() == Some(b'/') && self.peek2() == Some(b'*') {
                            depth += 1;
                            self.advance();
                            self.advance();
                        } else if self.peek() == Some(b'*') && self.peek2() == Some(b'/') {
                            depth -= 1;
                            self.advance();
                            self.advance();
                        } else {
                            self.advance();
                        }
                    }
                    if depth > 0 {
                        return Err(LexError {
                            msg: "unterminated block comment".into(),
                            line: self.line,
                            col: self.col,
                        });
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn read_string(&mut self) -> Result<String, LexError> {
        // assumes opening quote already consumed
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(LexError {
                        msg: "unterminated string".into(),
                        line: self.line,
                        col: self.col,
                    })
                }
                Some(b'"') => {
                    self.advance();
                    break;
                }
                Some(b'\\') => {
                    self.advance();
                    let esc = self.advance().ok_or(LexError {
                        msg: "unterminated escape".into(),
                        line: self.line,
                        col: self.col,
                    })?;
                    match esc {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'0' => out.push('\0'),
                        b'\\' => out.push('\\'),
                        b'"' => out.push('"'),
                        b'\'' => out.push('\''),
                        b'x' => {
                            let h1 = self.advance().unwrap_or(b'0');
                            let h2 = self.advance().unwrap_or(b'0');
                            let v = hex(h1) * 16 + hex(h2);
                            out.push(v as char);
                        }
                        _ => out.push(esc as char),
                    }
                }
                Some(c) => {
                    self.advance();
                    out.push(c as char);
                }
            }
        }
        Ok(out)
    }

    fn read_char(&mut self) -> Result<char, LexError> {
        // assumes opening quote consumed
        let c = match self.peek() {
            Some(b'\\') => {
                self.advance();
                let esc = self.advance().ok_or(LexError {
                    msg: "unterminated char".into(),
                    line: self.line,
                    col: self.col,
                })?;
                match esc {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'0' => '\0',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    _ => esc as char,
                }
            }
            Some(c) => {
                self.advance();
                c as char
            }
            None => {
                return Err(LexError {
                    msg: "unterminated char".into(),
                    line: self.line,
                    col: self.col,
                })
            }
        };
        if self.peek() != Some(b'\'') {
            return Err(LexError {
                msg: "expected closing ' for char literal".into(),
                line: self.line,
                col: self.col,
            });
        }
        self.advance();
        Ok(c)
    }

    fn read_number(&mut self) -> Result<Tok, LexError> {
        let mut s = String::new();
        let mut is_float = false;
        // hex / binary / octal
        if self.peek() == Some(b'0') {
            match self.peek2() {
                Some(b'x') | Some(b'X') => {
                    self.advance();
                    self.advance();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_hexdigit() {
                            s.push(c as char);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let v =
                        i64::from_str_radix(&s, 16).map_err(|e| LexError {
                            msg: format!("bad hex literal: {}", e),
                            line: self.line,
                            col: self.col,
                        })?;
                    return Ok(Tok::Int(v));
                }
                Some(b'b') | Some(b'B') => {
                    self.advance();
                    self.advance();
                    while let Some(c) = self.peek() {
                        if c == b'0' || c == b'1' {
                            s.push(c as char);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let v =
                        i64::from_str_radix(&s, 2).map_err(|e| LexError {
                            msg: format!("bad binary literal: {}", e),
                            line: self.line,
                            col: self.col,
                        })?;
                    return Ok(Tok::Int(v));
                }
                _ => {}
            }
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c as char);
                self.advance();
            } else if c == b'_' {
                self.advance(); // digit separator
            } else {
                break;
            }
        }
        if self.peek() == Some(b'.') && self.peek2().map_or(false, |c| c.is_ascii_digit()) {
            is_float = true;
            s.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c as char);
                    self.advance();
                } else if c == b'_' {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        if let Some(b'e') | Some(b'E') = self.peek() {
            is_float = true;
            s.push('e');
            self.advance();
            if let Some(b'+') | Some(b'-') = self.peek() {
                s.push(self.peek().unwrap() as char);
                self.advance();
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c as char);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        if is_float {
            let v: f64 = s.parse().map_err(|e| LexError {
                msg: format!("bad float literal: {}", e),
                line: self.line,
                col: self.col,
            })?;
            Ok(Tok::Float(v))
        } else {
            let v: i64 = s.parse().map_err(|e| LexError {
                msg: format!("bad int literal: {}", e),
                line: self.line,
                col: self.col,
            })?;
            Ok(Tok::Int(v))
        }
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' {
                s.push(c as char);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn classify_keyword(s: &str) -> Option<Tok> {
        Some(match s {
            "fn" => Tok::KwFn,
            "let" => Tok::KwLet,
            "mut" => Tok::KwMut,
            "if" => Tok::KwIf,
            "else" => Tok::KwElse,
            "elif" => Tok::KwElif,
            "while" => Tok::KwWhile,
            "for" => Tok::KwFor,
            "in" => Tok::KwIn,
            "return" => Tok::KwReturn,
            "break" => Tok::KwBreak,
            "continue" => Tok::KwContinue,
            "struct" => Tok::KwStruct,
            "enum" => Tok::KwEnum,
            "mod" => Tok::KwMod,
            "use" => Tok::KwUse,
            "macro" => Tok::KwMacro,
            "region" => Tok::KwRegion,
            "extern" => Tok::KwExtern,
            "match" => Tok::KwMatch,
            "as" => Tok::KwAs,
            "true" => Tok::KwTrue,
            "false" => Tok::KwFalse,
            "null" => Tok::KwNull,
            "and" => Tok::KwAnd,
            "or" => Tok::KwOr,
            "not" => Tok::KwNot,
            "int" => Tok::KwTypeInt,
            "float" => Tok::KwTypeFloat,
            "str" => Tok::KwTypeStr,
            "bool" => Tok::KwTypeBool,
            "char" => Tok::KwTypeChar,
            "void" => Tok::KwTypeVoid,
            _ => return None,
        })
    }

    pub fn next_token(&mut self) -> Result<Spanned, LexError> {
        self.skip_ws_and_comments()?;
        let start = self.pos;
        let line = self.line;
        let col = self.col;
        let c = match self.peek() {
            Some(c) => c,
            None => {
                return Ok(Spanned {
                    tok: Tok::Eof,
                    start,
                    end: start,
                    line,
                    col,
                })
            }
        };

        // Newline token — emitted as a soft statement separator.
        if c == b'\n' {
            self.advance();
            return Ok(Spanned {
                tok: Tok::Newline,
                start,
                end: self.pos,
                line,
                col,
            });
        }

        // Identifiers / keywords
        if c.is_ascii_alphabetic() || c == b'_' {
            let s = self.read_ident();
            let tok = if let Some(kw) = Self::classify_keyword(&s) {
                kw
            } else {
                Tok::Ident(s)
            };
            return Ok(Spanned {
                tok,
                start,
                end: self.pos,
                line,
                col,
            });
        }

        // Numbers
        if c.is_ascii_digit() {
            let tok = self.read_number()?;
            return Ok(Spanned {
                tok,
                start,
                end: self.pos,
                line,
                col,
            });
        }

        // Strings
        if c == b'"' {
            self.advance();
            let s = self.read_string()?;
            return Ok(Spanned {
                tok: Tok::Str(s),
                start,
                end: self.pos,
                line,
                col,
            });
        }
        if c == b'\'' {
            self.advance();
            let ch = self.read_char()?;
            return Ok(Spanned {
                tok: Tok::Char(ch),
                start,
                end: self.pos,
                line,
                col,
            });
        }

        // Punctuation & operators
        self.advance();
        let tok = match c {
            b'(' => Tok::LParen,
            b')' => Tok::RParen,
            b'{' => Tok::LBrace,
            b'}' => Tok::RBrace,
            b'[' => Tok::LBracket,
            b']' => Tok::RBracket,
            b',' => Tok::Comma,
            b'.' => Tok::Dot,
            b':' => {
                if self.peek() == Some(b':') {
                    self.advance();
                    Tok::DoubleColon
                } else {
                    Tok::Colon
                }
            }
            b';' => Tok::Semicolon,
            b'#' => Tok::Pound,
            b'@' => Tok::At,
            b'$' => Tok::Dollar,
            b'~' => Tok::BitNot,
            b'^' => Tok::BitXor,
            b'-' => match self.peek() {
                Some(b'>') => {
                    self.advance();
                    Tok::Arrow
                }
                Some(b'=') => {
                    self.advance();
                    Tok::MinusEq
                }
                _ => Tok::Minus,
            },
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::Eq
                } else if self.peek() == Some(b'>') {
                    self.advance();
                    Tok::FatArrow
                } else {
                    Tok::Assign
                }
            }
            b'+' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::PlusEq
                } else {
                    Tok::Plus
                }
            }
            b'*' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::StarEq
                } else {
                    Tok::Star
                }
            }
            b'/' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::SlashEq
                } else {
                    Tok::Slash
                }
            }
            b'%' => Tok::Percent,
            b'!' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::Neq
                } else {
                    Tok::KwNot
                }
            }
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::Le
                } else if self.peek() == Some(b'<') {
                    self.advance();
                    Tok::Shl
                } else {
                    Tok::Lt
                }
            }
            b'>' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::Ge
                } else if self.peek() == Some(b'>') {
                    self.advance();
                    Tok::Shr
                } else {
                    Tok::Gt
                }
            }
            b'&' => {
                if self.peek() == Some(b'&') {
                    self.advance();
                    Tok::KwAnd
                } else {
                    Tok::Ampersand
                }
            }
            b'|' => {
                if self.peek() == Some(b'|') {
                    self.advance();
                    Tok::KwOr
                } else if self.peek() == Some(b'=') {
                    self.advance();
                    Tok::BitOr
                } else {
                    Tok::Pipe
                }
            }
            other => {
                return Err(LexError {
                    msg: format!("unexpected char {:?}", other as char),
                    line,
                    col,
                })
            }
        };

        Ok(Spanned {
            tok,
            start,
            end: self.pos,
            line,
            col,
        })
    }

    pub fn lex_all(mut self) -> Result<Vec<Spanned>, LexError> {
        let mut out: Vec<Spanned> = Vec::new();
        loop {
            let t = self.next_token()?;
            let is_eof = matches!(t.tok, Tok::Eof);
            // Coalesce consecutive newlines into a single Newline.
            if matches!(t.tok, Tok::Newline) {
                if let Some(last) = out.last() {
                    if matches!(last.tok, Tok::Newline) {
                        continue;
                    }
                }
            }
            out.push(t);
            if is_eof {
                break;
            }
        }
        Ok(out)
    }
}

fn hex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}
