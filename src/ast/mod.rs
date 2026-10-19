//! Abstract Syntax Tree for Qessa.

use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}
impl Span {
    pub fn dummy() -> Self {
        Self { start: 0, end: 0, line: 0, col: 0 }
    }
}

// ---------------------------------------------------------------------------
// Types (used in optional annotations)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnn {
    Int,
    Float,
    Bool,
    Str,
    Char,
    Void,
    Named(String),
    Array(Box<TypeAnn>),
    Pointer(Box<TypeAnn>),
    Func(Vec<TypeAnn>, Box<TypeAnn>),
    Generic(String), // T
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal
    Int(i64, Span),
    Float(f64, Span),
    Str(Rc<str>, Span),
    Char(char, Span),
    Bool(bool, Span),
    Null(Span),

    /// Identifier reference
    Ident(String, Span),

    /// Array literal `[a, b, c]` (optional region: `in r`)
    ArrayLit(Vec<Expr>, Option<String>, Span),

    /// Struct literal `Foo { x: 1, y: 2 }`
    StructLit(String, Vec<(String, Expr)>, Span),

    /// Index `a[i]`
    Index(Box<Expr>, Box<Expr>, Span),

    /// Field access `a.b`
    Field(Box<Expr>, String, Span),

    /// Method call `a.b(c, d)`
    MethodCall(Box<Expr>, String, Vec<Expr>, Span),

    /// Function call `f(a, b)`
    Call(Box<Expr>, Vec<Expr>, Span),

    /// Binary op `a + b`
    BinaryOp(BinOp, Box<Expr>, Box<Expr>, Span),

    /// Unary op `-a`, `!a`, `*a`, `&a`
    UnaryOp(UnOp, Box<Expr>, Span),

    /// Assignment `a = b`
    Assign(Box<Expr>, Box<Expr>, Span),

    /// Compound assignment `a += b`
    CompoundAssign(BinOp, Box<Expr>, Box<Expr>, Span),

    /// Closure `|x, y| body` or `{ |x, y| body }`
    Closure(Vec<String>, Vec<Stmt>, Span),

    /// If-expression `if c { a } else { b }`
    IfExpr(Box<Expr>, Vec<Stmt>, Option<Vec<Stmt>>, Span),

    /// Match expression
    Match(Box<Expr>, Vec<(MatchPat, Vec<Stmt>)>, Span),

    /// Region expression `region r { body }` returns last value
    RegionExpr(String, Vec<Stmt>, Span),

    /// Allocate expression `new Type(args)` in given region
    New(String, Vec<Expr>, Option<String>, Span),

    /// Address-of `&x`
    AddrOf(Box<Expr>, Span),

    /// Dereference `*p`
    Deref(Box<Expr>, Span),

    /// Cast `e as T`
    Cast(Box<Expr>, TypeAnn, Span),

    /// Member of enum: `Color.Red`
    EnumVariant(String, String, Span),

    /// Tuple `(a, b, c)` — for returning multiple values
    Tuple(Vec<Expr>, Span),

    /// Macro invocation `name!(args)` — kept raw for the expander
    MacroCall(String, Vec<Expr>, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, s)
            | Expr::Float(_, s)
            | Expr::Str(_, s)
            | Expr::Char(_, s)
            | Expr::Bool(_, s)
            | Expr::Null(s)
            | Expr::Ident(_, s)
            | Expr::ArrayLit(.., s)
            | Expr::StructLit(.., s)
            | Expr::Index(.., s)
            | Expr::Field(.., s)
            | Expr::MethodCall(.., s)
            | Expr::Call(.., s)
            | Expr::BinaryOp(.., s)
            | Expr::UnaryOp(.., s)
            | Expr::Assign(.., s)
            | Expr::CompoundAssign(.., s)
            | Expr::Closure(.., s)
            | Expr::IfExpr(.., s)
            | Expr::Match(.., s)
            | Expr::RegionExpr(.., s)
            | Expr::New(.., s)
            | Expr::AddrOf(_, s)
            | Expr::Deref(_, s)
            | Expr::Cast(.., s)
            | Expr::EnumVariant(.., s)
            | Expr::Tuple(.., s)
            | Expr::MacroCall(.., s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Neq, Lt, Gt, Le, Ge,
    And, Or,
    BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg, Not, Deref, AddrOf,
}

#[derive(Debug, Clone)]
pub enum MatchPat {
    Wildcard,
    Int(i64),
    Str(String),
    Bool(bool),
    Null,
    Ident(String),
    EnumVariant(String, Option<String>),
    Tuple(Vec<MatchPat>),
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let name = expr;` (optional type ann, optional mut)
    Let(String, Option<TypeAnn>, bool, Option<Expr>, Span),

    /// Bare expression statement
    Expr(Expr, Span),

    /// `return expr;`
    Return(Option<Expr>, Span),

    /// `break;`
    Break(Span),
    /// `continue;`
    Continue(Span),

    /// `if c { ... } else if d { ... } else { ... }`
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>, Span),

    /// `while c { ... }`
    While(Expr, Vec<Stmt>, Span),

    /// `for x in iter { ... }`
    For(String, Expr, Vec<Stmt>, Span),

    /// `region r { ... }` (statement form)
    Region(String, Vec<Stmt>, Span),

    /// `match e { pat => body, ... }`
    Match(Expr, Vec<(MatchPat, Vec<Stmt>)>, Span),

    /// block `{ ... }`
    Block(Vec<Stmt>, Span),
}

// ---------------------------------------------------------------------------
// Top-level items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct FuncItem {
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: Option<TypeAnn>,
    pub body: Vec<Stmt>,
    pub is_extern: bool,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct StructItem {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<FieldDef>,
    pub methods: Vec<FuncItem>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariantDef {
    pub name: String,
    pub fields: Vec<FieldDef>, // for tagged enums
}

#[derive(Debug, Clone)]
pub struct EnumItem {
    pub name: String,
    pub variants: Vec<EnumVariantDef>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ModuleItem {
    pub name: String,
    pub items: Vec<Item>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MacroItem {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstItem {
    pub name: String,
    pub ty: Option<TypeAnn>,
    pub value: Expr,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Item {
    Func(FuncItem),
    Struct(StructItem),
    Enum(EnumItem),
    Module(ModuleItem),
    Macro(MacroItem),
    Const(ConstItem),
    /// `use foo::bar`
    Use(String, Vec<String>, Span),
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub items: Vec<Item>,
}
