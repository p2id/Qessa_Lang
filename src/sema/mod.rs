//! Semantic analysis: builds global symbol tables, resolves identifiers,
//! validates control flow, and lowers macros (a stub for now).
//!
//! Since the language is dynamically typed, we focus on:
//!  - resolving all top-level items into a flat namespace
//!  - collecting extern declarations (syscall numbers, etc.)
//!  - assigning unique IDs to functions for code generation
//!  - assigning frame slot indices to locals

use crate::ast::*;
use std::collections::HashMap;

#[derive(Debug)]
pub struct SemaError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}
impl std::fmt::Display for SemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sema error at {}:{}: {}", self.line, self.col, self.msg)
    }
}
impl std::error::Error for SemaError {}

#[derive(Debug, Clone)]
pub struct FuncInfo {
    pub name: String,
    pub qname: String,        // fully-qualified name
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub id: usize,            // function id for codegen
    pub is_extern: bool,
    pub ext_target: Option<String>, // syscall name or external symbol
    pub local_count: usize,
    pub param_count: usize,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub methods: HashMap<String, FuncItem>,
    pub field_offsets: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub variants: Vec<EnumVariantDef>,
    pub variant_tags: HashMap<String, u64>,
}

#[derive(Debug, Default)]
pub struct ProgramInfo {
    pub funcs: HashMap<String, FuncInfo>,  // qname -> info
    pub func_ids: Vec<String>,             // ordered list of qnames
    pub structs: HashMap<String, StructInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub consts: HashMap<String, i64>,
    pub string_literals: Vec<String>,      // dedup later in codegen
    pub entry: Option<String>,             // qname of `main`
    pub macros: HashMap<String, MacroItem>,
}

pub struct Sema {
    pub info: ProgramInfo,
    next_fn_id: usize,
    module_prefix: Vec<String>,
}

impl Sema {
    pub fn new() -> Self {
        Self {
            info: ProgramInfo::default(),
            next_fn_id: 0,
            module_prefix: Vec::new(),
        }
    }

    pub fn analyze(prog: &Program) -> Result<ProgramInfo, SemaError> {
        let mut s = Sema::new();
        s.collect_items(&prog.items)?;
        // pre-scan main
        s.info.entry = s.info.func_ids.iter().find(|n| n.as_str() == "main").cloned()
            .or_else(|| s.info.func_ids.iter().find(|n| n.ends_with("::main")).cloned());
        Ok(s.info)
    }

    fn qualify(&self, name: &str) -> String {
        if self.module_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.module_prefix.join("::"), name)
        }
    }

    fn collect_items(&mut self, items: &[Item]) -> Result<(), SemaError> {
        for it in items {
            self.collect_item(it)?;
        }
        Ok(())
    }

    fn collect_item(&mut self, it: &Item) -> Result<(), SemaError> {
        match it {
            Item::Func(f) => self.collect_func(f),
            Item::Struct(s) => self.collect_struct(s),
            Item::Enum(e) => self.collect_enum(e),
            Item::Module(m) => {
                self.module_prefix.push(m.name.clone());
                self.collect_items(&m.items)?;
                self.module_prefix.pop();
                Ok(())
            }
            Item::Macro(m) => {
                self.info.macros.insert(m.name.clone(), m.clone());
                Ok(())
            }
            Item::Const(c) => {
                if let Expr::Int(v, _) = &c.value {
                    self.info.consts.insert(self.qualify(&c.name), *v);
                }
                Ok(())
            }
            Item::Use(_, _, _) => Ok(()), // ignored for v1
        }
    }

    fn collect_func(&mut self, f: &FuncItem) -> Result<(), SemaError> {
        let qname = self.qualify(&f.name);
        if self.info.funcs.contains_key(&qname) {
            return Err(SemaError {
                msg: format!("duplicate function definition '{}'", qname),
                line: f.span.line,
                col: f.span.col,
            });
        }
        let id = self.next_fn_id;
        self.next_fn_id += 1;
        self.info.func_ids.push(qname.clone());

        // Count locals roughly by scanning body
        let local_count = count_locals(&f.body);
        let param_count = f.params.len();
        let ext_target = if f.is_extern {
            // For extern fn, the body is empty and the linkage name comes from parsing.
            // We treat all extern fns with the syscall_ prefix as syscall stubs.
            if f.name.starts_with("syscall_") {
                Some(f.name.clone())
            } else {
                Some(f.name.clone())
            }
        } else {
            None
        };

        let fi = FuncInfo {
            name: f.name.clone(),
            qname: qname.clone(),
            params: f.params.clone(),
            body: f.body.clone(),
            id,
            is_extern: f.is_extern,
            ext_target,
            local_count,
            param_count,
        };
        self.info.funcs.insert(qname, fi.clone());
        // Also register an alias under the unqualified name, so that
        // cross-module self-calls (e.g. `factorial` calling `factorial`)
        // resolve.  If a top-level function with the same name already
        // exists, we don't overwrite it.
        if !self.info.funcs.contains_key(&f.name) {
            self.info.funcs.insert(f.name.clone(), fi);
        }
        Ok(())
    }

    fn collect_struct(&mut self, s: &StructItem) -> Result<(), SemaError> {
        let qname = self.qualify(&s.name);
        let mut field_offsets = HashMap::new();
        for (i, fd) in s.fields.iter().enumerate() {
            field_offsets.insert(fd.name.clone(), i * 8);
        }
        let mut methods = HashMap::new();
        for m in &s.methods {
            methods.insert(m.name.clone(), m.clone());
        }
        self.info.structs.insert(qname.clone(), StructInfo {
            name: qname,
            fields: s.fields.clone(),
            methods,
            field_offsets,
        });
        Ok(())
    }

    fn collect_enum(&mut self, e: &EnumItem) -> Result<(), SemaError> {
        let qname = self.qualify(&e.name);
        let mut variant_tags = HashMap::new();
        for (i, v) in e.variants.iter().enumerate() {
            variant_tags.insert(v.name.clone(), i as u64);
        }
        self.info.enums.insert(qname.clone(), EnumInfo {
            name: qname,
            variants: e.variants.clone(),
            variant_tags,
        });
        Ok(())
    }
}

fn count_locals(stmts: &[Stmt]) -> usize {
    let mut n = 0;
    for s in stmts {
        n += count_locals_stmt(s);
    }
    n
}

fn count_locals_stmt(s: &Stmt) -> usize {
    match s {
        Stmt::Let(_, _, _, _, _) => 1,
        Stmt::Expr(e, _) => count_locals_expr(e),
        Stmt::Return(e, _) => e.as_ref().map_or(0, count_locals_expr),
        Stmt::If(c, t, e, _) => {
            count_locals_expr(c) + count_locals(t) + e.as_ref().map_or(0, |b| count_locals(b))
        }
        Stmt::While(c, b, _) => count_locals_expr(c) + count_locals(b),
        Stmt::For(_, e, b, _) => 1 + count_locals_expr(e) + count_locals(b),
        Stmt::Region(_, b, _) => 1 + count_locals(b),
        Stmt::Match(e, arms, _) => {
            count_locals_expr(e) + arms.iter().map(|(_, b)| count_locals(b)).sum::<usize>()
        }
        Stmt::Block(b, _) => count_locals(b),
        Stmt::Break(_) | Stmt::Continue(_) => 0,
    }
}

fn count_locals_expr(e: &Expr) -> usize {
    match e {
        Expr::ArrayLit(items, _, _) => 1 + items.iter().map(count_locals_expr).sum::<usize>(),
        Expr::StructLit(_, fields, _) => 1 + fields.iter().map(|(_, e)| count_locals_expr(e)).sum::<usize>(),
        Expr::Index(a, i, _) => count_locals_expr(a) + count_locals_expr(i),
        Expr::Field(a, _, _) => count_locals_expr(a),
        Expr::MethodCall(a, _, args, _) => count_locals_expr(a) + args.iter().map(count_locals_expr).sum::<usize>(),
        Expr::Call(f, args, _) => count_locals_expr(f) + args.iter().map(count_locals_expr).sum::<usize>(),
        Expr::BinaryOp(_, a, b, _) => count_locals_expr(a) + count_locals_expr(b),
        Expr::UnaryOp(_, a, _) => count_locals_expr(a),
        Expr::Assign(a, b, _) => count_locals_expr(a) + count_locals_expr(b),
        Expr::CompoundAssign(_, a, b, _) => count_locals_expr(a) + count_locals_expr(b),
        Expr::Closure(_, body, _) => 1 + count_locals(body),
        Expr::IfExpr(c, t, e, _) => count_locals_expr(c) + count_locals(t) + e.as_ref().map_or(0, |b| count_locals(b)),
        Expr::Match(e, arms, _) => count_locals_expr(e) + arms.iter().map(|(_, b)| count_locals(b)).sum::<usize>(),
        Expr::RegionExpr(_, b, _) => 1 + count_locals(b),
        Expr::New(_, args, _, _) => 1 + args.iter().map(count_locals_expr).sum::<usize>(),
        Expr::AddrOf(a, _) => count_locals_expr(a),
        Expr::Deref(a, _) => count_locals_expr(a),
        Expr::Cast(a, _, _) => count_locals_expr(a),
        Expr::Tuple(items, _) => items.iter().map(count_locals_expr).sum::<usize>(),
        Expr::MacroCall(_, args, _) => args.iter().map(count_locals_expr).sum::<usize>(),
        Expr::EnumVariant(_, _, _) => 0,
        Expr::Int(_, _) | Expr::Float(_, _) | Expr::Str(_, _) | Expr::Char(_, _)
        | Expr::Bool(_, _) | Expr::Null(_) | Expr::Ident(_, _) => 0,
    }
}
