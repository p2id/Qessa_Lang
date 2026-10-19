//! CLI subcommand implementations.

use crate::{ast::*, codegen::CodeGen, emit, lexer::Lexer, parser::Parser, sema::Sema};
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const EMBEDDED_STDLIB: &str = include_str!("../qessac/stdlib.qs");

pub fn help() {
    println!("Qessa compiler — direct-to-x86-64 machine code");
    println!();
    println!("USAGE:");
    println!("  qessac compile <input.qs> [-o <output>] [--no-stdlib]");
    println!("  qessac run     [--no-stdlib] <input.qs> [program args...]");
    println!("  qessac tokens  <input.qs>                 Dump tokens");
    println!("  qessac ast     <input.qs>                 Dump AST (debug)");
    println!("  qessac asm     <input.qs>                 Disassemble generated machine code");
    println!("  qessac help                              Show this help");
}

pub fn compile_cmd(args: &[String]) -> Result<(), String> {
    let (input, output, with_stdlib) = parse_io_args(args, "a.out")?;
    let src = std::fs::read_to_string(&input)
        .map_err(|e| format!("cannot read {}: {}", input, e))?;
    let elf = build_elf(&src, with_stdlib)?;
    std::fs::write(&output, &elf)
        .map_err(|e| format!("cannot write {}: {}", output, e))?;
    // chmod +x
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&output)
            .map_err(|e| format!("chmod metadata: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&output, perms)
            .map_err(|e| format!("chmod set: {}", e))?;
    }
    println!("Compiled {} -> {} ({} bytes)", input, output, elf.len());
    Ok(())
}

pub fn run_cmd(args: &[String]) -> Result<u8, String> {
    let mut with_stdlib = true;
    let mut input_index = None;
    for (i, arg) in args.iter().enumerate() {
        if input_index.is_none() && arg == "--no-stdlib" {
            with_stdlib = false;
        } else if input_index.is_none() {
            input_index = Some(i);
            break;
        }
    }
    let input_index = input_index.ok_or("missing input file")?;
    let input = &args[input_index];
    let src = std::fs::read_to_string(input)
        .map_err(|e| format!("cannot read {}: {}", input, e))?;
    let elf = build_elf(&src, with_stdlib)?;
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
    let mut tmp = None;
    let mut file = None;
    for _ in 0..100 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = std::env::temp_dir().join(format!(
            "qessa_run_{}_{}.bin", std::process::id(), id
        ));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(f) => { tmp = Some(candidate); file = Some(f); break; }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create temporary executable: {}", e)),
        }
    }
    let tmp = TemporaryExecutable(
        tmp.ok_or("could not allocate a unique temporary executable path")?,
    );
    file.take().ok_or("temporary executable handle was not created")?
        .write_all(&elf).map_err(|e| format!("cannot write temporary executable: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp.0)
            .map_err(|e| format!("{}", e))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp.0, perms).map_err(|e| format!("{}", e))?;
    }
    let status = Command::new(&tmp.0).args(&args[input_index + 1..]).status();
    let status = status.map_err(|e| format!("failed to execute: {}", e))?;
    if let Some(code) = status.code() {
        Ok(code.clamp(0, 255) as u8)
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            Err(format!("program terminated by signal {}", status.signal().unwrap_or(-1)))
        }
        #[cfg(not(unix))]
        { Err("program terminated without an exit code".to_string()) }
    }
}

pub fn tokens_cmd(args: &[String]) -> Result<(), String> {
    let input = args.get(0).ok_or("missing input file")?;
    let src = std::fs::read_to_string(input).map_err(|e| format!("{}", e))?;
    let toks = Lexer::new(&src).lex_all().map_err(|e| format!("{}", e))?;
    for t in &toks {
        println!("{:4}:{:3}  {:?}", t.line, t.col, t.tok);
    }
    Ok(())
}

pub fn ast_cmd(args: &[String]) -> Result<(), String> {
    let input = args.get(0).ok_or("missing input file")?;
    let src = std::fs::read_to_string(input).map_err(|e| format!("{}", e))?;
    let toks = Lexer::new(&src).lex_all().map_err(|e| format!("{}", e))?;
    let mut parser = Parser::new(toks);
    let prog = parser.parse_program().map_err(|e| format!("{}", e))?;
    print_program(&prog);
    Ok(())
}

pub fn asm_cmd(args: &[String]) -> Result<(), String> {
    let input = args.get(0).ok_or("missing input file")?;
    let src = std::fs::read_to_string(input).map_err(|e| format!("{}", e))?;
    let src = append_stdlib(&src, true);
    let toks = Lexer::new(&src).lex_all().map_err(|e| format!("{}", e))?;
    let mut parser = Parser::new(toks);
    let prog = parser.parse_program().map_err(|e| format!("{}", e))?;
    let info = Sema::analyze(&prog).map_err(|e| format!("{}", e))?;
    let mut cg = CodeGen::new(info);
    cg.compile();
    if !cg.errors.is_empty() {
        return Err(cg.errors.join("\n"));
    }
    println!("; === Text segment ({} bytes) ===", cg.mach.code.len());
    println!("; Source-line annotations: {}", cg.mach.lines.len());
    let mut last_line = 0;
    for (off, line) in &cg.mach.lines {
        if *line != last_line {
            println!("\n; --- source line {} ---", line);
            last_line = *line;
        }
        println!("{:06x}: ...", off);
    }
    println!("\n; === Strings ({} unique) ===", cg.strings.len());
    for (i, s) in cg.strings.iter().enumerate() {
        println!("  [{}] {:?}", i, s);
    }
    println!("\n; === Functions ({} defined) ===", cg.info.funcs.len());
    for q in &cg.info.func_ids {
        if let Some(f) = cg.info.funcs.get(q) {
            if !f.is_extern {
                let off = cg.func_offsets.get(&f.id).copied().unwrap_or(0);
                println!("  {:6}  fn {} (params={}, locals_est={})", off, q, f.param_count, f.local_count);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_io_args(args: &[String], default_out: &str) -> Result<(String, String, bool), String> {
    let mut input = None;
    let mut output = None;
    let mut with_stdlib = true;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                output = Some(args.get(i).cloned().ok_or("-o requires an output path")?);
            }
            "--no-stdlib" => {
                with_stdlib = false;
            }
            s if !s.starts_with('-') => {
                if input.replace(s.to_string()).is_some() {
                    return Err("unexpected extra input path".to_string());
                }
            }
            _ => return Err(format!("unknown flag: {}", args[i])),
        }
        i += 1;
    }
    let input = input.ok_or("missing input file")?;
    let output = output.unwrap_or_else(|| default_out.to_string());
    Ok((input, output, with_stdlib))
}

struct TemporaryExecutable(std::path::PathBuf);

impl Drop for TemporaryExecutable {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn append_stdlib(src: &str, with_stdlib: bool) -> String {
    if with_stdlib {
        format!("{}\n{}", src, EMBEDDED_STDLIB)
    } else {
        src.to_string()
    }
}

fn build_elf(src: &str, with_stdlib: bool) -> Result<Vec<u8>, String> {
    let src = append_stdlib(src, with_stdlib);
    let toks = Lexer::new(&src).lex_all().map_err(|e| format!("{}", e))?;
    let mut parser = Parser::new(toks);
    let prog = parser.parse_program().map_err(|e| format!("{}", e))?;
    let info = Sema::analyze(&prog).map_err(|e| format!("{}", e))?;
    if info.entry.is_none() {
        return Err("program must define an entry function `fn main`".to_string());
    }
    let mut cg = CodeGen::new(info);
    cg.compile();
    if !cg.errors.is_empty() {
        return Err(cg.errors.join("\n"));
    }
    let result = emit::emit(&cg);
    Ok(result.elf)
}

// ---------------------------------------------------------------------------
// AST pretty-printer (very compact)
// ---------------------------------------------------------------------------

fn print_program(p: &Program) {
    println!("Program:");
    for it in &p.items {
        print_item(it, 1);
    }
}

fn print_item(it: &Item, indent: usize) {
    let pad = "  ".repeat(indent);
    match it {
        Item::Func(f) => {
            println!("{}Func {} ({} params, extern={})", pad, f.name, f.params.len(), f.is_extern);
            for s in &f.body { print_stmt(s, indent + 1); }
        }
        Item::Struct(s) => {
            println!("{}Struct {} ({} fields, {} methods)", pad, s.name, s.fields.len(), s.methods.len());
        }
        Item::Enum(e) => {
            println!("{}Enum {} ({} variants)", pad, e.name, e.variants.len());
        }
        Item::Module(m) => {
            println!("{}Module {}", pad, m.name);
            for it in &m.items { print_item(it, indent + 1); }
        }
        Item::Macro(m) => {
            println!("{}Macro {} ({} params)", pad, m.name, m.params.len());
        }
        Item::Const(c) => {
            println!("{}Const {}", pad, c.name);
        }
        Item::Use(root, path, _) => {
            println!("{}Use {}::{}", pad, root, path.join("::"));
        }
    }
}

fn print_stmt(s: &Stmt, indent: usize) {
    let pad = "  ".repeat(indent);
    match s {
        Stmt::Let(n, _, _, _init, _) => println!("{}Let {}", pad, n),
        Stmt::Expr(e, _) => { print!("{}Expr: ", pad); print_expr(e, 0); println!(); }
        Stmt::Return(_e, _) => println!("{}Return", pad),
        Stmt::Break(_) => println!("{}Break", pad),
        Stmt::Continue(_) => println!("{}Continue", pad),
        Stmt::If(_, t, e, _) => {
            println!("{}If (then={}, else={})", pad, t.len(), e.as_ref().map_or(0, |b| b.len()));
            for s in t { print_stmt(s, indent + 1); }
        }
        Stmt::While(_, b, _) => {
            println!("{}While (body={})", pad, b.len());
            for s in b { print_stmt(s, indent + 1); }
        }
        Stmt::For(v, _, b, _) => {
            println!("{}For {}", pad, v);
            for s in b { print_stmt(s, indent + 1); }
        }
        Stmt::Region(n, b, _) => {
            println!("{}Region {}", pad, n);
            for s in b { print_stmt(s, indent + 1); }
        }
        Stmt::Match(_, arms, _) => println!("{}Match ({} arms)", pad, arms.len()),
        Stmt::Block(b, _) => {
            println!("{}Block", pad);
            for s in b { print_stmt(s, indent + 1); }
        }
    }
}

fn print_expr(e: &Expr, depth: usize) {
    let _ = depth;
    match e {
        Expr::Int(v, _) => print!("{}", v),
        Expr::Float(v, _) => print!("{}", v),
        Expr::Str(s, _) => print!("{:?}", s),
        Expr::Char(c, _) => print!("'{}'", c),
        Expr::Bool(b, _) => print!("{}", b),
        Expr::Null(_) => print!("null"),
        Expr::Ident(n, _) => print!("{}", n),
        Expr::ArrayLit(items, _, _) => print!("[{} items]", items.len()),
        Expr::StructLit(n, _, _) => print!("{}{{}}", n),
        Expr::Index(a, i, _) => { print!("Index("); print_expr(a, depth); print!(", "); print_expr(i, depth); print!(")"); }
        Expr::Field(a, n, _) => { print!("Field("); print_expr(a, depth); print!(".{})", n); }
        Expr::MethodCall(_, n, args, _) => print!("method {}({} args)", n, args.len()),
        Expr::Call(callee, args, _) => {
            print!("Call(");
            print_expr(callee, depth + 1);
            print!(", {} args)", args.len());
        }
        Expr::BinaryOp(op, a, b, _) => { print!("BinOp({:?}, ", op); print_expr(a, depth); print!(", "); print_expr(b, depth); print!(")"); }
        Expr::UnaryOp(op, a, _) => { print!("UnOp({:?}, ", op); print_expr(a, depth); print!(")"); }
        Expr::Assign(l, r, _) => { print!("Assign("); print_expr(l, depth); print!(", "); print_expr(r, depth); print!(")"); }
        Expr::CompoundAssign(op, l, r, _) => { print!("CompoundAssign({:?}, ", op); print_expr(l, depth); print!(", "); print_expr(r, depth); print!(")"); }
        Expr::Closure(params, _, _) => print!("Closure({} params)", params.len()),
        Expr::IfExpr(_, _, _, _) => print!("IfExpr"),
        Expr::Match(_, arms, _) => print!("Match({} arms)", arms.len()),
        Expr::RegionExpr(n, _, _) => print!("Region({})", n),
        Expr::New(n, args, _, _) => print!("New {}({} args)", n, args.len()),
        Expr::AddrOf(a, _) => { print!("AddrOf("); print_expr(a, depth); print!(")"); }
        Expr::Deref(a, _) => { print!("Deref("); print_expr(a, depth); print!(")"); }
        Expr::Cast(_, _, _) => print!("Cast"),
        Expr::EnumVariant(e, v, _) => print!("{}::{}", e, v),
        Expr::Tuple(items, _) => print!("Tuple({})", items.len()),
        Expr::MacroCall(n, args, _) => print!("MacroCall {}({})", n, args.len()),
    }
}
