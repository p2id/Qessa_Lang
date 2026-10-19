//! Recursive-descent parser for Qessa.

use crate::ast::*;
use crate::lexer::{Lexer, Spanned, Tok};
use std::rc::Rc;

pub struct Parser {
    toks: Vec<Spanned>,
    pos: usize,
}

#[derive(Debug)]
pub struct ParseError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error at {}:{}: {}", self.line, self.col, self.msg)
    }
}
impl std::error::Error for ParseError {}

macro_rules! pe {
    ($self:ident, $($arg:tt)*) => {
        return Err(ParseError { msg: format!($($arg)*), line: $self.cur().line, col: $self.cur().col })
    };
}

impl Parser {
    pub fn new(toks: Vec<Spanned>) -> Self {
        Self { toks, pos: 0 }
    }

    pub fn from_source(src: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let toks = Lexer::new(src).lex_all()?;
        Ok(Self::new(toks))
    }

    fn cur(&self) -> &Spanned {
        &self.toks[self.pos]
    }
    fn peek(&self, off: usize) -> &Spanned {
        &self.toks[(self.pos + off).min(self.toks.len() - 1)]
    }
    fn at(&self, t: &Tok) -> bool {
        std::mem::discriminant(&self.cur().tok) == std::mem::discriminant(t)
    }
    fn bump(&mut self) -> Spanned {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: Tok) -> Result<Spanned, ParseError> {
        if std::mem::discriminant(&self.cur().tok) == std::mem::discriminant(&t) {
            Ok(self.bump())
        } else {
            Err(ParseError {
                msg: format!("expected {:?}, got {:?}", t, self.cur().tok),
                line: self.cur().line,
                col: self.cur().col,
            })
        }
    }
    fn try_eat(&mut self, t: Tok) -> Option<Spanned> {
        if std::mem::discriminant(&self.cur().tok) == std::mem::discriminant(&t) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn span_here(&self) -> Span {
        let s = self.cur();
        Span { start: s.start, end: s.end, line: s.line, col: s.col }
    }

    /// Skip any Newline tokens.  Used at statement boundaries and inside
    /// expressions where line breaks are insignificant.
    fn skip_newlines(&mut self) {
        while self.at(&Tok::Newline) {
            self.bump();
        }
    }

    fn skip_stmt_separators(&mut self) {
        while self.at(&Tok::Newline) || self.at(&Tok::Semicolon) {
            self.bump();
        }
    }

    /// Like `at`, but skips leading newlines first.
    fn at_after_nl(&mut self, t: &Tok) -> bool {
        self.skip_newlines();
        self.at(t)
    }

    /// Like `eat`, but skips leading newlines first.
    fn eat_after_nl(&mut self, t: Tok) -> Result<Spanned, ParseError> {
        self.skip_newlines();
        self.eat(t)
    }

    // -----------------------------------------------------------------------
    // Top level
    // -----------------------------------------------------------------------

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at(&Tok::Eof) {
            let item = self.parse_item(false)?;
            items.push(item);
            self.skip_newlines();
        }
        Ok(Program { items })
    }

    fn parse_modifiers(&mut self) -> (bool, bool) {
        // returns (is_pub, is_extern)
        let mut is_pub = false;
        let mut is_extern = false;
        loop {
            match &self.cur().tok {
                Tok::Ident(s) if s == "pub" => {
                    is_pub = true;
                    self.bump();
                }
                Tok::KwExtern => {
                    is_extern = true;
                    self.bump();
                }
                _ => break,
            }
        }
        (is_pub, is_extern)
    }

    fn parse_item(&mut self, _in_module: bool) -> Result<Item, ParseError> {
        let (is_pub, is_extern) = self.parse_modifiers();
        let span = self.span_here();
        match self.cur().tok.clone() {
            Tok::KwFn => self.parse_func(is_pub, is_extern, span),
            Tok::KwStruct => self.parse_struct(is_pub, span),
            Tok::KwEnum => self.parse_enum(is_pub, span),
            Tok::KwMod => self.parse_module(is_pub, span),
            Tok::KwMacro => self.parse_macro(span),
            Tok::KwUse => self.parse_use(span),
            Tok::Ident(s) if s == "const" => {
                self.bump();
                self.parse_const(is_pub, span)
            }
            Tok::KwLet => {
                // Top-level let → treat as const
                self.bump();
                self.parse_const(is_pub, span)
            }
            other => pe!(self, "expected item, got {:?}", other),
        }
    }

    fn parse_func(&mut self, is_pub: bool, is_extern: bool, span: Span) -> Result<Item, ParseError> {
        self.eat(Tok::KwFn)?;
        let name = self.parse_ident()?;
        let generics = self.parse_generic_params()?;
        self.eat(Tok::LParen)?;
        let params = self.parse_params()?;
        self.eat(Tok::RParen)?;
        let ret = if self.try_eat(Tok::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = if is_extern {
            // extern fn: no body, optional `= "symbol"`
            if self.try_eat(Tok::Assign).is_some() {
                // skip the linkage name; we just store body empty
                if let Tok::Str(_) = self.cur().tok {
                    self.bump();
                }
            }
            self.try_eat(Tok::Semicolon);
            Vec::new()
        } else {
            self.eat(Tok::LBrace)?;
            let b = self.parse_block_stmts()?;
            self.eat(Tok::RBrace)?;
            b
        };
        Ok(Item::Func(FuncItem {
            name,
            generics,
            params,
            ret,
            body,
            is_extern,
            is_pub,
            span,
        }))
    }

    fn parse_generic_params(&mut self) -> Result<Vec<String>, ParseError> {
        let mut out = Vec::new();
        if self.try_eat(Tok::Lt).is_some() {
            while !self.at(&Tok::Gt) {
                let n = self.parse_ident()?;
                out.push(n);
                if self.try_eat(Tok::Comma).is_none() {
                    break;
                }
            }
            self.eat(Tok::Gt)?;
        }
        Ok(out)
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut out = Vec::new();
        while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
            let name = self.parse_ident()?;
            let ty = if self.try_eat(Tok::Colon).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            let default = if self.try_eat(Tok::Assign).is_some() {
                Some(self.parse_expr()?)
            } else {
                None
            };
            out.push(Param { name, ty, default });
            if self.try_eat(Tok::Comma).is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn parse_struct(&mut self, is_pub: bool, span: Span) -> Result<Item, ParseError> {
        self.eat(Tok::KwStruct)?;
        let name = self.parse_ident()?;
        let generics = self.parse_generic_params()?;
        self.eat(Tok::LBrace)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        self.skip_newlines();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            // detect `fn` for method
            if self.at(&Tok::KwFn) {
                let m_span = self.span_here();
                let m = self.parse_func(true, false, m_span)?;
                if let Item::Func(f) = m {
                    methods.push(f);
                }
                self.skip_newlines();
                continue;
            }
            let fname = self.parse_ident()?;
            let ty = if self.try_eat(Tok::Colon).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            let default = if self.try_eat(Tok::Assign).is_some() {
                Some(self.parse_expr()?)
            } else {
                None
            };
            fields.push(FieldDef { name: fname, ty, default });
            self.try_eat(Tok::Comma);
            self.skip_newlines();
        }
        self.eat(Tok::RBrace)?;
        Ok(Item::Struct(StructItem { name, generics, fields, methods, is_pub, span }))
    }

    fn parse_enum(&mut self, is_pub: bool, span: Span) -> Result<Item, ParseError> {
        self.eat(Tok::KwEnum)?;
        let name = self.parse_ident()?;
        self.eat(Tok::LBrace)?;
        let mut variants = Vec::new();
        self.skip_newlines();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            let vname = self.parse_ident()?;
            let mut fields = Vec::new();
            if self.try_eat(Tok::LBrace).is_some() {
                self.skip_newlines();
                while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
                    let fname = self.parse_ident()?;
                    let ty = if self.try_eat(Tok::Colon).is_some() {
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    fields.push(FieldDef { name: fname, ty, default: None });
                    self.try_eat(Tok::Comma);
                    self.skip_newlines();
                }
                self.eat(Tok::RBrace)?;
            }
            variants.push(EnumVariantDef { name: vname, fields });
            self.try_eat(Tok::Comma);
            self.skip_newlines();
        }
        self.eat(Tok::RBrace)?;
        Ok(Item::Enum(EnumItem { name, variants, is_pub, span }))
    }

    fn parse_module(&mut self, is_pub: bool, span: Span) -> Result<Item, ParseError> {
        self.eat(Tok::KwMod)?;
        let name = self.parse_ident()?;
        self.eat(Tok::LBrace)?;
        let mut items = Vec::new();
        self.skip_newlines();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            items.push(self.parse_item(true)?);
            self.skip_newlines();
        }
        self.eat(Tok::RBrace)?;
        Ok(Item::Module(ModuleItem { name, items, is_pub, span }))
    }

    fn parse_macro(&mut self, span: Span) -> Result<Item, ParseError> {
        self.eat(Tok::KwMacro)?;
        let name = self.parse_ident()?;
        self.eat(Tok::LParen)?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
            params.push(self.parse_ident()?);
            if self.try_eat(Tok::Comma).is_none() {
                break;
            }
        }
        self.eat(Tok::RParen)?;
        self.eat(Tok::LBrace)?;
        let body = self.parse_block_stmts()?;
        self.eat(Tok::RBrace)?;
        Ok(Item::Macro(MacroItem { name, params, body, span }))
    }

    fn parse_use(&mut self, span: Span) -> Result<Item, ParseError> {
        self.eat(Tok::KwUse)?;
        let mut path = Vec::new();
        let first = self.parse_ident()?;
        path.push(first);
        while self.try_eat(Tok::DoubleColon).is_some() {
            path.push(self.parse_ident()?);
        }
        let root = path.remove(0);
        self.try_eat(Tok::Semicolon);
        Ok(Item::Use(root, path, span))
    }

    fn parse_const(&mut self, is_pub: bool, span: Span) -> Result<Item, ParseError> {
        let name = self.parse_ident()?;
        let ty = if self.try_eat(Tok::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.eat(Tok::Assign)?;
        let value = self.parse_expr()?;
        self.try_eat(Tok::Semicolon);
        Ok(Item::Const(ConstItem { name, ty, value, is_pub, span }))
    }

    fn parse_ident(&mut self) -> Result<String, ParseError> {
        match self.bump().tok {
            Tok::Ident(s) => Ok(s),
            other => Err(ParseError {
                msg: format!("expected identifier, got {:?}", other),
                line: self.cur().line,
                col: self.cur().col,
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------

    fn parse_type(&mut self) -> Result<TypeAnn, ParseError> {
        match self.cur().tok.clone() {
            Tok::KwTypeInt => { self.bump(); Ok(TypeAnn::Int) }
            Tok::KwTypeFloat => { self.bump(); Ok(TypeAnn::Float) }
            Tok::KwTypeBool => { self.bump(); Ok(TypeAnn::Bool) }
            Tok::KwTypeStr => { self.bump(); Ok(TypeAnn::Str) }
            Tok::KwTypeChar => { self.bump(); Ok(TypeAnn::Char) }
            Tok::KwTypeVoid => { self.bump(); Ok(TypeAnn::Void) }
            Tok::Ident(s) => {
                self.bump();
                // could be generic
                if self.at(&Tok::Lt) {
                    self.bump();
                    let _ = self.parse_type()?;
                    while self.try_eat(Tok::Comma).is_some() {
                        let _ = self.parse_type()?;
                    }
                    self.eat(Tok::Gt)?;
                }
                Ok(TypeAnn::Named(s))
            }
            Tok::LBracket => {
                self.bump();
                let inner = self.parse_type()?;
                self.eat(Tok::RBracket)?;
                Ok(TypeAnn::Array(Box::new(inner)))
            }
            Tok::Ampersand => {
                self.bump();
                let inner = self.parse_type()?;
                Ok(TypeAnn::Pointer(Box::new(inner)))
            }
            Tok::LParen => {
                self.bump();
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                    args.push(self.parse_type()?);
                    if self.try_eat(Tok::Comma).is_none() { break; }
                }
                self.eat(Tok::RParen)?;
                self.eat(Tok::Arrow)?;
                let ret = self.parse_type()?;
                Ok(TypeAnn::Func(args, Box::new(ret)))
            }
            other => pe!(self, "expected type, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    fn parse_block_stmts(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut out = Vec::new();
        self.skip_stmt_separators();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            let s = self.parse_stmt()?;
            out.push(s);
            self.skip_stmt_separators();
        }
        Ok(out)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let span = self.span_here();
        match self.cur().tok.clone() {
            Tok::KwLet => {
                self.bump();
                let is_mut = self.try_eat(Tok::KwMut).is_some();
                let name = self.parse_ident()?;
                let ty = if self.try_eat(Tok::Colon).is_some() {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                let init = if self.try_eat(Tok::Assign).is_some() {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::Let(name, ty, is_mut, init, span))
            }
            Tok::KwReturn => {
                self.bump();
                let e = if self.at(&Tok::Semicolon) || self.at(&Tok::Newline) || self.at(&Tok::RBrace) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::Return(e, span))
            }
            Tok::KwBreak => { self.bump(); self.try_eat(Tok::Semicolon); Ok(Stmt::Break(span)) }
            Tok::KwContinue => { self.bump(); self.try_eat(Tok::Semicolon); Ok(Stmt::Continue(span)) }
            Tok::KwIf => {
                let (cond, then_b, else_b) = self.parse_if_chain()?;
                Ok(Stmt::If(cond, then_b, else_b, span))
            }
            Tok::KwWhile => {
                self.bump();
                let cond = self.parse_expr()?;
                self.eat(Tok::LBrace)?;
                let body = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::While(cond, body, span))
            }
            Tok::KwFor => {
                self.bump();
                let var = self.parse_ident()?;
                self.eat(Tok::KwIn)?;
                let iter = self.parse_expr()?;
                self.eat(Tok::LBrace)?;
                let body = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::For(var, iter, body, span))
            }
            Tok::KwRegion => {
                self.bump();
                let name = self.parse_ident()?;
                self.eat(Tok::LBrace)?;
                let body = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::Region(name, body, span))
            }
            Tok::KwMatch => {
                self.bump();
                let e = self.parse_expr()?;
                self.eat(Tok::LBrace)?;
                let mut arms = Vec::new();
                self.skip_newlines();
                while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
                    let pat = self.parse_pattern()?;
                    self.eat(Tok::FatArrow)?;
                    self.eat(Tok::LBrace)?;
                    let body = self.parse_block_stmts()?;
                    self.eat(Tok::RBrace)?;
                    arms.push((pat, body));
                    self.try_eat(Tok::Comma);
                    self.skip_newlines();
                }
                self.eat(Tok::RBrace)?;
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::Match(e, arms, span))
            }
            Tok::LBrace => {
                self.bump();
                let body = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                Ok(Stmt::Block(body, span))
            }
            _ => {
                let e = self.parse_expr()?;
                self.try_eat(Tok::Semicolon);
                Ok(Stmt::Expr(e, span))
            }
        }
    }

    fn parse_if_chain(&mut self) -> Result<(Expr, Vec<Stmt>, Option<Vec<Stmt>>), ParseError> {
        self.eat(Tok::KwIf)?;
        let cond = self.parse_expr()?;
        self.eat(Tok::LBrace)?;
        let then_b = self.parse_block_stmts()?;
        self.eat(Tok::RBrace)?;
        let else_b = if self.try_eat(Tok::KwElse).is_some() {
            if self.at(&Tok::KwIf) {
                let (c2, t2, e2) = self.parse_if_chain()?;
                Some(vec![Stmt::If(c2, t2, e2, Span::dummy())])
            } else {
                self.eat(Tok::LBrace)?;
                let b = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                Some(b)
            }
        } else {
            None
        };
        Ok((cond, then_b, else_b))
    }

    fn parse_pattern(&mut self) -> Result<MatchPat, ParseError> {
        match self.cur().tok.clone() {
            Tok::Ident(s) if s == "_" => { self.bump(); Ok(MatchPat::Wildcard) }
            Tok::Int(v) => { self.bump(); Ok(MatchPat::Int(v)) }
            Tok::Str(s) => { self.bump(); Ok(MatchPat::Str(s)) }
            Tok::KwTrue => { self.bump(); Ok(MatchPat::Bool(true)) }
            Tok::KwFalse => { self.bump(); Ok(MatchPat::Bool(false)) }
            Tok::KwNull => { self.bump(); Ok(MatchPat::Null) }
            Tok::Ident(s) => {
                self.bump();
                // Collect path segments: a::b::c::...
                let mut segments = vec![s];
                while self.try_eat(Tok::DoubleColon).is_some() {
                    segments.push(self.parse_ident()?);
                }
                if segments.len() == 1 {
                    Ok(MatchPat::Ident(segments.into_iter().next().unwrap()))
                } else {
                    // Last segment = variant, second-to-last = enum name
                    let variant = segments.pop().unwrap();
                    let enum_name = segments.pop().unwrap();
                    Ok(MatchPat::EnumVariant(enum_name, Some(variant)))
                }
            }
            Tok::LParen => {
                self.bump();
                let mut pats = Vec::new();
                while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                    pats.push(self.parse_pattern()?);
                    if self.try_eat(Tok::Comma).is_none() { break; }
                }
                self.eat(Tok::RParen)?;
                Ok(MatchPat::Tuple(pats))
            }
            other => pe!(self, "expected pattern, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Expressions (Pratt-style precedence)
    // -----------------------------------------------------------------------

    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> Result<Expr, ParseError> {
        let lhs = self.parse_or()?;
        let span = lhs.span();
        match self.cur().tok {
            Tok::Assign => {
                self.bump();
                let rhs = self.parse_assign()?;
                Ok(Expr::Assign(Box::new(lhs), Box::new(rhs), span))
            }
            Tok::PlusEq => {
                self.bump();
                let rhs = self.parse_assign()?;
                Ok(Expr::CompoundAssign(BinOp::Add, Box::new(lhs), Box::new(rhs), span))
            }
            Tok::MinusEq => {
                self.bump();
                let rhs = self.parse_assign()?;
                Ok(Expr::CompoundAssign(BinOp::Sub, Box::new(lhs), Box::new(rhs), span))
            }
            Tok::StarEq => {
                self.bump();
                let rhs = self.parse_assign()?;
                Ok(Expr::CompoundAssign(BinOp::Mul, Box::new(lhs), Box::new(rhs), span))
            }
            Tok::SlashEq => {
                self.bump();
                let rhs = self.parse_assign()?;
                Ok(Expr::CompoundAssign(BinOp::Div, Box::new(lhs), Box::new(rhs), span))
            }
            _ => Ok(lhs),
        }
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and()?;
        while let Tok::KwOr = self.cur().tok {
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr::BinaryOp(BinOp::Or, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_bitor()?;
        while let Tok::KwAnd = self.cur().tok {
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_bitor()?;
            lhs = Expr::BinaryOp(BinOp::And, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_bitor(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_bitxor()?;
        while let Tok::Pipe = self.cur().tok {
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_bitxor()?;
            lhs = Expr::BinaryOp(BinOp::BitOr, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_bitxor(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_bitand()?;
        while let Tok::BitXor = self.cur().tok {
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_bitand()?;
            lhs = Expr::BinaryOp(BinOp::BitXor, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_bitand(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_eq()?;
        while let Tok::Ampersand = self.cur().tok {
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_eq()?;
            lhs = Expr::BinaryOp(BinOp::BitAnd, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_eq(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_cmp()?;
        loop {
            let op = match self.cur().tok {
                Tok::Eq => BinOp::Eq,
                Tok::Neq => BinOp::Neq,
                _ => break,
            };
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_cmp()?;
            lhs = Expr::BinaryOp(op, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_shift()?;
        loop {
            let op = match self.cur().tok {
                Tok::Lt => BinOp::Lt,
                Tok::Gt => BinOp::Gt,
                Tok::Le => BinOp::Le,
                Tok::Ge => BinOp::Ge,
                _ => break,
            };
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_shift()?;
            lhs = Expr::BinaryOp(op, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.cur().tok {
                Tok::Shl => BinOp::Shl,
                Tok::Shr => BinOp::Shr,
                _ => break,
            };
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_add()?;
            lhs = Expr::BinaryOp(op, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.cur().tok {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_mul()?;
            lhs = Expr::BinaryOp(op, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.cur().tok {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            let s = self.span_here();
            self.bump();
            let rhs = self.parse_unary()?;
            lhs = Expr::BinaryOp(op, Box::new(lhs), Box::new(rhs), s);
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let span = self.span_here();
        match self.cur().tok {
            Tok::Minus => {
                self.bump();
                let e = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnOp::Neg, Box::new(e), span))
            }
            Tok::KwNot => {
                self.bump();
                let e = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnOp::Not, Box::new(e), span))
            }
            Tok::Star => {
                self.bump();
                let e = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnOp::Deref, Box::new(e), span))
            }
            Tok::Ampersand => {
                self.bump();
                let e = self.parse_unary()?;
                Ok(Expr::UnaryOp(UnOp::AddrOf, Box::new(e), span))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.parse_primary()?;
        loop {
            let span = self.span_here();
            match self.cur().tok {
                Tok::LParen => {
                    self.bump();
                    let mut args = Vec::new();
                    while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                        args.push(self.parse_expr()?);
                        if self.try_eat(Tok::Comma).is_none() { break; }
                    }
                    self.eat(Tok::RParen)?;
                    e = Expr::Call(Box::new(e), args, span);
                }
                Tok::LBracket => {
                    self.bump();
                    let idx = self.parse_expr()?;
                    self.eat(Tok::RBracket)?;
                    e = Expr::Index(Box::new(e), Box::new(idx), span);
                }
                Tok::Dot => {
                    self.bump();
                    let name = self.parse_ident()?;
                    if self.at(&Tok::LParen) {
                        self.bump();
                        let mut args = Vec::new();
                        while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                            args.push(self.parse_expr()?);
                            if self.try_eat(Tok::Comma).is_none() { break; }
                        }
                        self.eat(Tok::RParen)?;
                        e = Expr::MethodCall(Box::new(e), name, args, span);
                    } else if self.at(&Tok::DoubleColon) {
                        // Enum::Variant access via dot? no - use ::
                        e = Expr::Field(Box::new(e), name, span);
                    } else {
                        e = Expr::Field(Box::new(e), name, span);
                    }
                }
                Tok::DoubleColon => {
                    self.bump();
                    let name = self.parse_ident()?;
                    // Handle path: collect all `::` segments.
                    // For `a::b::c::d`, we produce EnumVariant("a::b::c", "d").
                    // For `a::b`, we produce EnumVariant("a", "b").
                    let mut path = vec![name];
                    while self.try_eat(Tok::DoubleColon).is_some() {
                        path.push(self.parse_ident()?);
                    }
                    // Build the qualified enum name from the leading segments.
                    let variant = path.pop().unwrap();
                    let qualifier = if let Expr::Ident(s, _) = &e {
                        // Original e was just an Ident; prepend it.
                        let mut all = vec![s.clone()];
                        all.extend(path);
                        all.join("::")
                    } else if let Expr::EnumVariant(base, _, _) = &e {
                        // Already partially-qualified; extend the base.
                        let mut all = vec![base.clone()];
                        all.extend(path);
                        all.join("::")
                    } else {
                        path.join("::")
                    };
                    e = Expr::EnumVariant(qualifier, variant, span);
                }
                Tok::KwAs => {
                    self.bump();
                    let t = self.parse_type()?;
                    e = Expr::Cast(Box::new(e), t, span);
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let span = self.span_here();
        match self.cur().tok.clone() {
            Tok::Int(v) => { self.bump(); Ok(Expr::Int(v, span)) }
            Tok::Float(v) => { self.bump(); Ok(Expr::Float(v, span)) }
            Tok::Str(s) => { self.bump(); Ok(Expr::Str(Rc::from(s.as_str()), span)) }
            Tok::Char(c) => { self.bump(); Ok(Expr::Char(c, span)) }
            Tok::KwTrue => { self.bump(); Ok(Expr::Bool(true, span)) }
            Tok::KwFalse => { self.bump(); Ok(Expr::Bool(false, span)) }
            Tok::KwNull => { self.bump(); Ok(Expr::Null(span)) }
            Tok::Ident(s) if s == "new" => {
                self.bump();
                let tname = self.parse_ident()?;
                self.eat(Tok::LParen)?;
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                    args.push(self.parse_expr()?);
                    if self.try_eat(Tok::Comma).is_none() { break; }
                }
                self.eat(Tok::RParen)?;
                let region = if self.at(&Tok::KwIn) {
                    self.bump();
                    Some(self.parse_ident()?)
                } else {
                    None
                };
                Ok(Expr::New(tname, args, region, span))
            }
            Tok::Ident(s) => {
                self.bump();
                // macro invocation? `name!(...)`
                if self.at(&Tok::Pound) {
                    self.bump();
                    self.eat(Tok::LParen)?;
                    let mut args = Vec::new();
                    while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                        args.push(self.parse_expr()?);
                        if self.try_eat(Tok::Comma).is_none() { break; }
                    }
                    self.eat(Tok::RParen)?;
                    return Ok(Expr::MacroCall(s, args, span));
                }
                // struct literal: `Foo { ... }`
                if self.at(&Tok::LBrace) && self.looks_like_struct_lit() {
                    self.bump();
                    let mut fields = Vec::new();
                    while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
                        let fname = self.parse_ident()?;
                        let val = if self.try_eat(Tok::Colon).is_some() {
                            self.parse_expr()?
                        } else {
                            Expr::Ident(fname.clone(), span.clone())
                        };
                        fields.push((fname, val));
                        if self.try_eat(Tok::Comma).is_none() { break; }
                    }
                    self.eat(Tok::RBrace)?;
                    return Ok(Expr::StructLit(s, fields, span));
                }
                Ok(Expr::Ident(s, span))
            }
            Tok::LParen => {
                self.bump();
                if self.try_eat(Tok::RParen).is_some() {
                    return Ok(Expr::Tuple(Vec::new(), span));
                }
                let first = self.parse_expr()?;
                if self.try_eat(Tok::Comma).is_some() {
                    let mut items = vec![first];
                    while !self.at(&Tok::RParen) && !self.at(&Tok::Eof) {
                        items.push(self.parse_expr()?);
                        if self.try_eat(Tok::Comma).is_none() { break; }
                    }
                    self.eat(Tok::RParen)?;
                    return Ok(Expr::Tuple(items, span));
                }
                self.eat(Tok::RParen)?;
                Ok(first)
            }
            Tok::LBracket => {
                self.bump();
                let mut items = Vec::new();
                while !self.at(&Tok::RBracket) && !self.at(&Tok::Eof) {
                    items.push(self.parse_expr()?);
                    if self.try_eat(Tok::Comma).is_none() { break; }
                }
                self.eat(Tok::RBracket)?;
                let region = if self.at(&Tok::KwIn) {
                    self.bump();
                    Some(self.parse_ident()?)
                } else {
                    None
                };
                Ok(Expr::ArrayLit(items, region, span))
            }
            Tok::LBrace => {
                self.bump();
                // closure detection: `|args| body`
                if self.at(&Tok::Pipe) {
                    self.bump();
                    let mut params = Vec::new();
                    while !self.at(&Tok::Pipe) && !self.at(&Tok::Eof) {
                        params.push(self.parse_ident()?);
                        if self.try_eat(Tok::Comma).is_none() { break; }
                    }
                    self.eat(Tok::Pipe)?;
                    let body = self.parse_block_stmts()?;
                    self.eat(Tok::RBrace)?;
                    return Ok(Expr::Closure(params, body, span));
                }
                let body = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                // A bare block expression — treat as `if true { body }`.
                Ok(Expr::IfExpr(Box::new(Expr::Bool(true, span.clone())), body, None, span))
            }
            Tok::Pipe => {
                // bare closure outside of braces
                self.bump();
                let mut params = Vec::new();
                while !self.at(&Tok::Pipe) && !self.at(&Tok::Eof) {
                    params.push(self.parse_ident()?);
                    if self.try_eat(Tok::Comma).is_none() { break; }
                }
                self.eat(Tok::Pipe)?;
                // single-expr body until comma/semicolon/rbrace
                let body_expr = self.parse_expr()?;
                return Ok(Expr::Closure(params, vec![Stmt::Expr(body_expr, span.clone())], span));
            }
            Tok::KwIf => {
                let (cond, then_b, else_b) = self.parse_if_chain()?;
                Ok(Expr::IfExpr(Box::new(cond), then_b, else_b, span))
            }
            Tok::KwMatch => {
                self.bump();
                let e = self.parse_expr()?;
                self.eat(Tok::LBrace)?;
                let mut arms = Vec::new();
                while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
                    let pat = self.parse_pattern()?;
                    self.eat(Tok::FatArrow)?;
                    let body = self.parse_block_stmts()?;
                    arms.push((pat, body));
                    self.try_eat(Tok::Comma);
                }
                self.eat(Tok::RBrace)?;
                Ok(Expr::Match(Box::new(e), arms, span))
            }
            Tok::KwRegion => {
                self.bump();
                let name = self.parse_ident()?;
                self.eat(Tok::LBrace)?;
                let body = self.parse_block_stmts()?;
                self.eat(Tok::RBrace)?;
                Ok(Expr::RegionExpr(name, body, span))
            }
            other => pe!(self, "unexpected token in expression: {:?}", other),
        }
    }

    fn looks_like_struct_lit(&self) -> bool {
        // after `Ident LBrace`, we look for `Ident :` or `Ident ,` or `}`
        if !self.at(&Tok::LBrace) {
            return false;
        }
        let next = self.peek(1);
        match &next.tok {
            Tok::RBrace => true,
            Tok::Ident(_) => {
                let after = self.peek(2);
                matches!(after.tok, Tok::Colon | Tok::Comma | Tok::RBrace)
            }
            _ => false,
        }
    }
}
