//! Qessa compiler CLI.
//!
//! Usage:
//!   qessac compile <input.qs> -o <output>      Compile to a Linux ELF executable
//!   qessac run     <input.qs>                  Compile + execute immediately
//!   qessac tokens  <input.qs>                  Dump tokens
//!   qessac ast     <input.qs>                  Dump AST (debug)
//!   qessac asm     <input.qs>                  Disassemble generated machine code
//!   qessac help

use std::process::ExitCode;

mod lexer;
mod parser;
mod ast;
mod sema;
mod codegen;
mod emit;
mod cli;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: qessac <command> [args]");
        eprintln!("commands: compile, run, tokens, ast, asm, help");
        return ExitCode::from(1);
    }
    let cmd = args[1].as_str();
    let rest = &args[2..];
    let result: Result<ExitCode, String> = match cmd {
        "compile" => cli::compile_cmd(rest).map(|()| ExitCode::SUCCESS),
        "run"     => cli::run_cmd(rest).map(ExitCode::from),
        "tokens"  => cli::tokens_cmd(rest).map(|()| ExitCode::SUCCESS),
        "ast"     => cli::ast_cmd(rest).map(|()| ExitCode::SUCCESS),
        "asm"     => cli::asm_cmd(rest).map(|()| ExitCode::SUCCESS),
        "help" | "--help" | "-h" => { cli::help(); Ok(ExitCode::SUCCESS) }
        other => Err(format!("unknown command: {}", other)),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(1)
        }
    }
}
