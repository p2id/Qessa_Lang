//! Top-level code generator.
//!
//! Lowers the AST directly to x86-64 machine code using a simple
//! stack-machine evaluation model with RAX as the accumulator.
//!
//! Value model (all values are 8-byte i64s):
//!   * integers: their value
//!   * booleans: 0 or 1
//!   * null:     0
//!   * pointers: raw addresses into the heap or .rodata
//!
//! Stack-frame layout (per function, low → high addresses):
//!   [ saved RBP           ]   <- RBP points here
//!   [ return address      ]
//!   [ caller's stack ...  ]
//!   Local slots are *below* RBP, at negative offsets.

use crate::ast::*;
use crate::codegen::mach::{cc, reg, Mach};
use crate::sema::{FuncInfo, ProgramInfo};
use std::collections::HashMap;

pub mod mach;

pub struct CodeGen {
    pub mach: Mach,
    pub info: ProgramInfo,
    pub func_offsets: HashMap<usize, usize>,
    pub call_patches: Vec<(usize, String)>,
    /// Patches for runtime helper calls — resolved in ELF emitter.
    pub runtime_patches: Vec<(usize, String)>,
    /// Patches for string literal `lea rax, [rip+disp]` — resolved later.
    pub string_patches: Vec<(usize, usize)>, // (mach-code offset, string idx)
    /// Patches for `lea rax, [rip+disp]` taking address of a function —
    /// resolved later.
    pub fn_ptr_patches: Vec<(usize, String)>,
    pub strings: Vec<String>,
    pub string_index: HashMap<String, usize>,
    pub rodata: Vec<u8>,
    cur_fn: Option<usize>,
    locals: HashMap<String, i32>,
    next_local_offset: i32,
    loop_breaks: Vec<Vec<usize>>,
    loop_continues: Vec<Vec<usize>>,
    regions: HashMap<String, i32>,
    max_temps: i32,
    pub errors: Vec<String>,
}

impl CodeGen {
    pub fn new(info: ProgramInfo) -> Self {
        Self {
            mach: Mach::new(),
            info,
            func_offsets: HashMap::new(),
            call_patches: Vec::new(),
            runtime_patches: Vec::new(),
            string_patches: Vec::new(),
            fn_ptr_patches: Vec::new(),
            strings: Vec::new(),
            string_index: HashMap::new(),
            rodata: Vec::new(),
            cur_fn: None,
            locals: HashMap::new(),
            next_local_offset: 0,
            loop_breaks: Vec::new(),
            loop_continues: Vec::new(),
            regions: HashMap::new(),
            max_temps: 0,
            errors: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // String interning
    // -----------------------------------------------------------------------
    fn intern_str(&mut self, s: &str) -> usize {
        if let Some(&i) = self.string_index.get(s) {
            return i;
        }
        let i = self.strings.len();
        self.strings.push(s.to_string());
        self.string_index.insert(s.to_string(), i);
        i
    }

    /// Returns the offset of each string within the rodata blob (built after
    /// all codegen is done, so we know the total count and contents).
    pub fn string_offsets(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.strings.len());
        let mut off = 0;
        for s in &self.strings {
            out.push(off);
            off += s.len() + 1;
        }
        out
    }

    // -----------------------------------------------------------------------
    // Local slot management
    // -----------------------------------------------------------------------
    fn alloc_local(&mut self, name: &str) -> i32 {
        self.next_local_offset += 8;
        self.locals.insert(name.to_string(), -self.next_local_offset);
        -self.next_local_offset
    }
    fn alloc_temp(&mut self) -> i32 {
        self.next_local_offset += 8;
        let off = -self.next_local_offset;
        if self.next_local_offset > self.max_temps {
            self.max_temps = self.next_local_offset;
        }
        off
    }
    fn lookup_local(&self, name: &str) -> Option<i32> {
        self.locals.get(name).copied()
    }

    // -----------------------------------------------------------------------
    // Main entry: compile all functions
    // -----------------------------------------------------------------------
    pub fn compile(&mut self) {
        // Single pass: assign offset, then immediately emit code for each fn.
        let qnames: Vec<String> = self.info.func_ids.clone();
        for q in &qnames {
            if let Some(fi) = self.info.funcs.get(q).cloned() {
                if fi.is_extern { continue; }
                let offset = self.mach.pos();
                self.func_offsets.insert(fi.id, offset);
                self.compile_fn(&fi);
            }
        }
        // Build rodata blob.
        self.build_rodata();
    }

    fn build_rodata(&mut self) {
        self.rodata.clear();
        for s in &self.strings {
            self.rodata.extend_from_slice(s.as_bytes());
            self.rodata.push(0);
        }
    }

    // -----------------------------------------------------------------------
    // Function prologue / epilogue
    // -----------------------------------------------------------------------
    fn compile_fn(&mut self, fi: &FuncInfo) {
        self.cur_fn = Some(fi.id);
        self.locals.clear();
        self.next_local_offset = 0;
        self.max_temps = 0;
        self.regions.clear();
        self.loop_breaks.clear();
        self.loop_continues.clear();

        // prologue: push rbp; mov rbp, rsp
        self.mach.push_r(reg::RBP);
        self.mach.mov_rr(reg::RBP, reg::RSP);

        // Reserve local space — patch later.
        // sub rsp, imm32
        self.mach.put(0x48); self.mach.put(0x81); self.mach.put(0xEC);
        let patch_local_size = self.mach.pos();
        self.mach.put_i32(0);

        // Store parameters from registers into locals.
        for (i, p) in fi.params.iter().enumerate() {
            let off = self.alloc_local(&p.name);
            if i < 6 {
                self.mach.mov_mr(reg::RBP, off, reg::ARGS[i]);
            } else {
                let stack_off = 16 + (i as i32 - 6) * 8;
                self.mach.mov_rm(reg::RAX, reg::RBP, stack_off);
                self.mach.mov_mr(reg::RBP, off, reg::RAX);
            }
        }

        // Compile body.
        for s in &fi.body {
            self.compile_stmt(s);
        }

        // Implicit `return 0` at end of function.
        self.mach.mov_ri32(reg::RAX, 0);
        // Epilogue: leave; ret
        self.mach.put(0xC9); // leave
        self.mach.put(0xC3); // ret

        // Patch the local-size reservation. 16-byte align.
        let total = self.next_local_offset.max(self.max_temps);
        let total = (total + 15) & !15;
        let bytes = (total as i32).to_le_bytes();
        self.mach.code[patch_local_size..patch_local_size + 4].copy_from_slice(&bytes);

        self.cur_fn = None;
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------
    fn compile_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(name, _ty, _is_mut, init, span) => {
                self.mach.line(span.line);
                let off = self.alloc_local(name);
                if let Some(e) = init {
                    self.compile_expr(e);
                    self.mach.mov_mr(reg::RBP, off, reg::RAX);
                } else {
                    self.mach.mov_ri32(reg::RAX, 0);
                    self.mach.mov_mr(reg::RBP, off, reg::RAX);
                }
            }
            Stmt::Expr(e, span) => {
                self.mach.line(span.line);
                self.compile_expr(e);
            }
            Stmt::Return(e, span) => {
                self.mach.line(span.line);
                if let Some(e) = e {
                    self.compile_expr(e);
                } else {
                    self.mach.mov_ri32(reg::RAX, 0);
                }
                self.mach.put(0xC9); // leave
                self.mach.put(0xC3); // ret
            }
            Stmt::Break(span) => {
                self.mach.line(span.line);
                let p = self.mach.jmp_rel32();
                if let Some(top) = self.loop_breaks.last_mut() {
                    top.push(p);
                }
            }
            Stmt::Continue(span) => {
                self.mach.line(span.line);
                let p = self.mach.jmp_rel32();
                if let Some(top) = self.loop_continues.last_mut() {
                    top.push(p);
                }
            }
            Stmt::If(cond, then_b, else_b, span) => {
                self.mach.line(span.line);
                self.compile_expr(cond);
                self.mach.test_rr(reg::RAX, reg::RAX);
                let jmp_else = self.mach.jcc_rel32(cc::Z);
                for s in then_b { self.compile_stmt(s); }
                let jmp_end = self.mach.jmp_rel32();
                let else_target = self.mach.pos();
                self.mach.patch_rel32(jmp_else, else_target);
                if let Some(else_b) = else_b {
                    for s in else_b { self.compile_stmt(s); }
                }
                let end = self.mach.pos();
                self.mach.patch_rel32(jmp_end, end);
            }
            Stmt::While(cond, body, span) => {
                self.mach.line(span.line);
                let start = self.mach.pos();
                self.compile_expr(cond);
                self.mach.test_rr(reg::RAX, reg::RAX);
                let jmp_end = self.mach.jcc_rel32(cc::Z);
                self.loop_breaks.push(Vec::new());
                self.loop_continues.push(Vec::new());
                for s in body { self.compile_stmt(s); }
                let back = self.mach.jmp_rel32();
                self.mach.patch_rel32(back, start);
                let end = self.mach.pos();
                self.mach.patch_rel32(jmp_end, end);
                if let Some(breaks) = self.loop_breaks.pop() {
                    for p in breaks { self.mach.patch_rel32(p, end); }
                }
                if let Some(continues) = self.loop_continues.pop() {
                    for p in continues { self.mach.patch_rel32(p, start); }
                }
            }
            Stmt::For(var, iter, body, span) => {
                self.mach.line(span.line);
                let arr_off = self.alloc_local("__for_arr");
                let n_off = self.alloc_local("__for_n");
                let i_off = self.alloc_local("__for_i");
                let var_off = self.alloc_local(var);

                self.compile_expr(iter);
                self.mach.mov_mr(reg::RBP, arr_off, reg::RAX);
                // load length: RAX = [arr_off]
                self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
                self.mach.mov_rm(reg::RAX, reg::RAX, 0);
                self.mach.mov_mr(reg::RBP, n_off, reg::RAX);
                self.mach.mov_ri32(reg::RAX, 0);
                self.mach.mov_mr(reg::RBP, i_off, reg::RAX);

                let start = self.mach.pos();
                self.mach.mov_rm(reg::RAX, reg::RBP, i_off);
                self.mach.mov_rm(reg::RCX, reg::RBP, n_off);
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                let jmp_end = self.mach.jcc_rel32(cc::NL);

                self.loop_breaks.push(Vec::new());
                self.loop_continues.push(Vec::new());

                // var = arr[i+2] (skip length+capacity = 16 bytes)
                self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
                self.mach.mov_rm(reg::RCX, reg::RBP, i_off);
                self.mach.lea_rm(reg::RAX, reg::RAX, 16);
                self.mach.mov_ri32(reg::RDX, 8);
                self.mach.imul_rr(reg::RDX, reg::RCX);
                self.mach.add_rr(reg::RAX, reg::RDX);
                self.mach.mov_rm(reg::RAX, reg::RAX, 0);
                self.mach.mov_mr(reg::RBP, var_off, reg::RAX);

                for s in body { self.compile_stmt(s); }

                // i++
                self.mach.mov_rm(reg::RAX, reg::RBP, i_off);
                self.mach.add_ri(reg::RAX, 1);
                self.mach.mov_mr(reg::RBP, i_off, reg::RAX);

                let back = self.mach.jmp_rel32();
                self.mach.patch_rel32(back, start);
                let end = self.mach.pos();
                self.mach.patch_rel32(jmp_end, end);

                if let Some(breaks) = self.loop_breaks.pop() {
                    for p in breaks { self.mach.patch_rel32(p, end); }
                }
                if let Some(continues) = self.loop_continues.pop() {
                    for p in continues { self.mach.patch_rel32(p, self.mach.pos()); }
                }
            }
            Stmt::Region(name, body, span) => {
                self.mach.line(span.line);
                let region_off = self.alloc_local(name);
                self.regions.insert(name.clone(), region_off);

                self.call_runtime("__region_create");
                self.mach.mov_mr(reg::RBP, region_off, reg::RAX);

                for s in body { self.compile_stmt(s); }

                self.mach.mov_rm(reg::RAX, reg::RBP, region_off);
                self.mach.mov_rr(reg::RDI, reg::RAX);
                self.call_runtime("__region_destroy");
            }
            Stmt::Match(e, arms, span) => {
                self.mach.line(span.line);
                self.compile_expr(e);
                let scrut_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, scrut_off, reg::RAX);

                let mut end_patches = Vec::new();
                for (pat, body) in arms {
                    self.mach.mov_rm(reg::RAX, reg::RBP, scrut_off);
                    self.compile_pat_check(pat);
                    self.mach.test_rr(reg::RAX, reg::RAX);
                    let next = self.mach.jcc_rel32(cc::Z);
                    if let MatchPat::Ident(name) = pat {
                        let off = self.alloc_local(name);
                        self.mach.mov_rm(reg::RAX, reg::RBP, scrut_off);
                        self.mach.mov_mr(reg::RBP, off, reg::RAX);
                    }
                    for s in body { self.compile_stmt(s); }
                    let jmp_end = self.mach.jmp_rel32();
                    end_patches.push(jmp_end);
                    let target = self.mach.pos();
                    self.mach.patch_rel32(next, target);
                }
                let end = self.mach.pos();
                for p in end_patches { self.mach.patch_rel32(p, end); }
            }
            Stmt::Block(b, _) => {
                for s in b { self.compile_stmt(s); }
            }
        }
    }

    fn compile_pat_check(&mut self, pat: &MatchPat) {
        match pat {
            MatchPat::Wildcard | MatchPat::Ident(_) => {
                self.mach.mov_ri32(reg::RAX, 1);
            }
            MatchPat::Int(v) => {
                self.mach.mov_ri32(reg::RCX, *v as i32);
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::Z, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            MatchPat::Bool(b) => {
                self.mach.mov_ri32(reg::RCX, *b as i32);
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::Z, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            MatchPat::Null => {
                self.mach.test_rr(reg::RAX, reg::RAX);
                self.mach.setcc_r(cc::Z, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            MatchPat::EnumVariant(enum_name, variant) => {
                let enum_tag = string_hash(enum_name) & 0xFF;
                let var_idx = if let Some(v) = variant {
                    self.enum_variant_index(enum_name, v)
                } else {
                    0
                };
                let expected = ((enum_tag << 56) | (var_idx & 0x00FFFFFFFFFFFFFF)) as i64;
                self.mach.mov_ri64(reg::RCX, expected);
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::Z, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            MatchPat::Str(_) => {
                // QES-SEC-002: string patterns used to match any non-null value.
                self.errors.push("unsupported string pattern in match (not implemented)".to_string());
                self.mach.mov_ri32(reg::RAX, 0);
            }
            MatchPat::Tuple(_) => {
                self.errors.push("unsupported tuple pattern in match (not implemented)".to_string());
                self.mach.mov_ri32(reg::RAX, 0);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Expressions: result always in RAX
    // -----------------------------------------------------------------------
    fn compile_expr(&mut self, e: &Expr) {
        match e {
            Expr::Int(v, span) => {
                self.mach.line(span.line);
                if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                    self.mach.mov_ri32(reg::RAX, *v as i32);
                } else {
                    self.mach.mov_ri64(reg::RAX, *v);
                }
            }
            Expr::Float(v, span) => {
                self.mach.line(span.line);
                let bits = v.to_bits() as i64;
                self.mach.mov_ri64(reg::RAX, bits);
            }
            Expr::Bool(b, span) => {
                self.mach.line(span.line);
                self.mach.mov_ri32(reg::RAX, *b as i32);
            }
            Expr::Null(span) => {
                self.mach.line(span.line);
                self.mach.mov_ri32(reg::RAX, 0);
            }
            Expr::Char(c, span) => {
                self.mach.line(span.line);
                self.mach.mov_ri32(reg::RAX, *c as i32);
            }
            Expr::Str(s, span) => {
                self.mach.line(span.line);
                let idx = self.intern_str(s);
                // lea rax, [rip+disp32]
                self.mach.put(0x48); self.mach.put(0x8D); self.mach.put(0x05);
                let p = self.mach.pos();
                self.mach.put_i32(0);
                self.string_patches.push((p, idx));
            }
            Expr::Ident(name, span) => {
                self.mach.line(span.line);
                if let Some(off) = self.lookup_local(name) {
                    self.mach.mov_rm(reg::RAX, reg::RBP, off);
                } else if let Some(&v) = self.info.consts.get(name) {
                    self.mach.mov_ri32(reg::RAX, v as i32);
                } else if self.info.funcs.contains_key(name) {
                    self.mach.put(0x48); self.mach.put(0x8D); self.mach.put(0x05);
                    let p = self.mach.pos();
                    self.mach.put_i32(0);
                    self.fn_ptr_patches.push((p, name.clone()));
                } else {
                    self.errors.push(format!("{}:{}: unknown identifier '{}'", span.line, span.col, name));
                    self.mach.mov_ri32(reg::RAX, 0);
                }
            }
            Expr::ArrayLit(items, region, span) => {
                self.mach.line(span.line);
                self.mach.mov_ri32(reg::RDI, items.len() as i32);
                if let Some(rname) = region {
                    if let Some(roff) = self.regions.get(rname) {
                        self.mach.mov_rm(reg::RSI, reg::RBP, *roff);
                    } else {
                        self.mach.mov_ri32(reg::RSI, 0);
                    }
                } else {
                    self.mach.mov_ri32(reg::RSI, 0);
                }
                self.call_runtime("__alloc_array");
                let arr_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, arr_off, reg::RAX);
                for (i, item) in items.iter().enumerate() {
                    self.compile_expr(item);
                    self.mach.mov_rm(reg::RCX, reg::RBP, arr_off);
                    self.mach.mov_mr(reg::RCX, 16 + (i as i32 * 8), reg::RAX);
                }
                self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
            }
            Expr::Index(arr, idx, span) => {
                self.mach.line(span.line);
                self.compile_expr(arr);
                let arr_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, arr_off, reg::RAX);
                self.compile_expr(idx);
                self.mach.mov_rr(reg::RCX, reg::RAX);
                self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
                self.mach.lea_rm(reg::RAX, reg::RAX, 16);
                self.mach.mov_ri32(reg::RDX, 8);
                self.mach.imul_rr(reg::RDX, reg::RCX);
                self.mach.add_rr(reg::RAX, reg::RDX);
                self.mach.mov_rm(reg::RAX, reg::RAX, 0);
            }
            Expr::Field(obj, name, span) => {
                self.mach.line(span.line);
                self.compile_expr(obj);
                let off = self.field_offset_for(name);
                self.mach.mov_rm(reg::RAX, reg::RAX, off as i32);
            }
            Expr::MethodCall(obj, name, args, span) => {
                self.mach.line(span.line);
                // Lower to runtime dispatch: __method_<name>(obj, args...)
                let mut arg_offs = Vec::new();
                self.compile_expr(obj);
                let off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, off, reg::RAX);
                arg_offs.push(off);
                for a in args {
                    self.compile_expr(a);
                    let off = self.alloc_temp();
                    self.mach.mov_mr(reg::RBP, off, reg::RAX);
                    arg_offs.push(off);
                }
                for (i, off) in arg_offs.iter().take(6).enumerate() {
                    self.mach.mov_rm(reg::ARGS[i], reg::RBP, *off);
                }
                let stack_args = arg_offs.len().saturating_sub(6);
                let stack_padding = if stack_args % 2 == 1 { 8 } else { 0 };
                if stack_padding != 0 {
                    self.mach.sub_ri(reg::RSP, stack_padding);
                }
                for off in arg_offs.iter().skip(6).rev() {
                    self.mach.mov_rm(reg::RAX, reg::RBP, *off);
                    self.mach.push_r(reg::RAX);
                }
                let fname = format!("__method_{}", name);
                self.call_runtime(&fname);
                if stack_args > 0 {
                    self.mach.add_ri(reg::RSP, stack_args as i32 * 8 + stack_padding);
                }
            }
            Expr::Call(callee, args, span) => {
                self.mach.line(span.line);
                let mut arg_offs = Vec::new();
                for a in args {
                    self.compile_expr(a);
                    let off = self.alloc_temp();
                    self.mach.mov_mr(reg::RBP, off, reg::RAX);
                    arg_offs.push(off);
                }
                for (i, off) in arg_offs.iter().take(6).enumerate() {
                    self.mach.mov_rm(reg::ARGS[i], reg::RBP, *off);
                }
                let stack_args = arg_offs.len().saturating_sub(6);
                let stack_padding = if stack_args % 2 == 1 { 8 } else { 0 };
                if stack_padding != 0 {
                    self.mach.sub_ri(reg::RSP, stack_padding);
                }
                for off in arg_offs.iter().skip(6).rev() {
                    self.mach.mov_rm(reg::RAX, reg::RBP, *off);
                    self.mach.push_r(reg::RAX);
                }
                match &**callee {
                    Expr::Ident(name, _) => {
                        if name.starts_with("syscall_") {
                            if let Some(num) = syscall_number(name) {
                                if arg_offs.len() > 6 {
                                    self.errors.push(format!("{}:{}: syscall accepts at most six arguments", span.line, span.col));
                                    self.mach.ud2();
                                } else {
                                    if arg_offs.len() >= 4 {
                                        self.mach.mov_rr(reg::R10, reg::RCX);
                                    }
                                    self.mach.mov_ri32(reg::RAX, num as i32);
                                    self.mach.syscall();
                                }
                            } else {
                                self.errors.push(format!("{}:{}: unknown syscall '{}'", span.line, span.col, name));
                                self.mach.ud2();
                            }
                        } else if let Some(fi) = self.info.funcs.get(name) {
                            if fi.is_extern {
                                self.errors.push(format!("{}:{}: external symbol '{}' cannot be linked into a static Qessa executable", span.line, span.col, name));
                                self.mach.ud2();
                            } else {
                                let p = self.mach.call_rel32();
                                self.call_patches.push((p, fi.qname.clone()));
                            }
                        } else if is_runtime_helper(name) {
                            let p = self.mach.call_rel32();
                            self.runtime_patches.push((p, name.clone()));
                        } else {
                            self.errors.push(format!("{}:{}: unknown function '{}'", span.line, span.col, name));
                            self.mach.ud2();
                        }
                    }
                    Expr::EnumVariant(mod_name, fn_name, _) => {
                        // Qualified call: mod::fn(args)
                        let qname = format!("{}::{}", mod_name, fn_name);
                        if let Some(fi) = self.info.funcs.get(&qname) {
                            if fi.is_extern {
                                self.errors.push(format!("{}:{}: external symbol '{}' cannot be linked into a static Qessa executable", span.line, span.col, qname));
                                self.mach.ud2();
                            } else {
                                let p = self.mach.call_rel32();
                                self.call_patches.push((p, fi.qname.clone()));
                            }
                        } else {
                            self.errors.push(format!("{}:{}: unknown function '{}'", span.line, span.col, qname));
                            self.mach.ud2();
                        }
                    }
                    _ => {
                        self.compile_expr(callee);
                        self.mach.call_r(reg::RAX);
                    }
                }
                if stack_args > 0 {
                    self.mach.add_ri(reg::RSP, stack_args as i32 * 8 + stack_padding);
                }
            }
            Expr::BinaryOp(op, a, b, span) => {
                self.mach.line(span.line);
                self.compile_binary(*op, a, b);
            }
            Expr::UnaryOp(op, a, span) => {
                self.mach.line(span.line);
                self.compile_unary(*op, a);
            }
            Expr::Assign(lhs, rhs, span) => {
                self.mach.line(span.line);
                self.compile_expr(rhs);
                self.store_to(lhs);
            }
            Expr::CompoundAssign(op, lhs, rhs, span) => {
                self.mach.line(span.line);
                self.load_from(lhs);
                let l_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, l_off, reg::RAX);
                self.compile_expr(rhs);
                self.mach.mov_rr(reg::RCX, reg::RAX);
                self.mach.mov_rm(reg::RAX, reg::RBP, l_off);
                self.emit_binop(*op);
                self.store_to(lhs);
            }
            Expr::IfExpr(cond, then_b, else_b, span) => {
                self.mach.line(span.line);
                self.compile_expr(cond);
                self.mach.test_rr(reg::RAX, reg::RAX);
                let jmp_else = self.mach.jcc_rel32(cc::Z);
                for s in then_b { self.compile_stmt(s); }
                let jmp_end = self.mach.jmp_rel32();
                let else_target = self.mach.pos();
                self.mach.patch_rel32(jmp_else, else_target);
                if let Some(else_b) = else_b {
                    for s in else_b { self.compile_stmt(s); }
                } else {
                    self.mach.mov_ri32(reg::RAX, 0);
                }
                let end = self.mach.pos();
                self.mach.patch_rel32(jmp_end, end);
            }
            Expr::RegionExpr(name, body, span) => {
                self.mach.line(span.line);
                let region_off = self.alloc_local(name);
                self.regions.insert(name.clone(), region_off);
                self.call_runtime("__region_create");
                self.mach.mov_mr(reg::RBP, region_off, reg::RAX);
                for s in body { self.compile_stmt(s); }
                self.mach.mov_rm(reg::RAX, reg::RBP, region_off);
                self.mach.mov_rr(reg::RDI, reg::RAX);
                self.call_runtime("__region_destroy");
            }
            Expr::New(tname, args, region, span) => {
                self.mach.line(span.line);
                let size = self.struct_size(tname);
                self.mach.mov_ri32(reg::RDI, size as i32);
                if let Some(rname) = region {
                    if let Some(roff) = self.regions.get(rname) {
                        self.mach.mov_rm(reg::RSI, reg::RBP, *roff);
                    } else {
                        self.mach.mov_ri32(reg::RSI, 0);
                    }
                } else {
                    self.mach.mov_ri32(reg::RSI, 0);
                }
                self.call_runtime("__alloc_struct");
                let off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, off, reg::RAX);
                for (i, a) in args.iter().enumerate() {
                    self.compile_expr(a);
                    self.mach.mov_rm(reg::RCX, reg::RBP, off);
                    self.mach.mov_mr(reg::RCX, i as i32 * 8, reg::RAX);
                }
                self.mach.mov_rm(reg::RAX, reg::RBP, off);
            }
            Expr::AddrOf(inner, span) => {
                self.mach.line(span.line);
                match &**inner {
                    Expr::Ident(name, _) => {
                        if let Some(off) = self.lookup_local(name) {
                            self.mach.lea_rm(reg::RAX, reg::RBP, off);
                        } else {
                            self.mach.mov_ri32(reg::RAX, 0);
                        }
                    }
                    Expr::Deref(p, _) => {
                        self.compile_expr(p);
                    }
                    Expr::Field(obj, fname, _) => {
                        self.compile_expr(obj);
                        let off = self.field_offset_for(fname);
                        self.mach.add_ri(reg::RAX, off as i32);
                    }
                    Expr::Index(arr, idx, _) => {
                        // &arr[idx] — pointer to the element
                        self.compile_expr(arr);
                        let arr_off = self.alloc_temp();
                        self.mach.mov_mr(reg::RBP, arr_off, reg::RAX);
                        self.compile_expr(idx);
                        self.mach.mov_rr(reg::RCX, reg::RAX);
                        self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
                        self.mach.lea_rm(reg::RAX, reg::RAX, 16);
                        self.mach.mov_ri32(reg::RDX, 8);
                        self.mach.imul_rr(reg::RDX, reg::RCX);
                        self.mach.add_rr(reg::RAX, reg::RDX);
                    }
                    _ => {
                        self.mach.mov_ri32(reg::RAX, 0);
                    }
                }
            }
            Expr::Deref(p, span) => {
                self.mach.line(span.line);
                self.compile_expr(p);
                self.mach.mov_rm(reg::RAX, reg::RAX, 0);
            }
            Expr::Cast(e, _ty, span) => {
                self.mach.line(span.line);
                self.compile_expr(e);
            }
            Expr::EnumVariant(enum_name, variant, span) => {
                self.mach.line(span.line);
                let enum_tag = string_hash(enum_name) & 0xFF;
                let var_idx = self.enum_variant_index(enum_name, variant);
                let v = ((enum_tag as u64) << 56) | (var_idx & 0x00FFFFFFFFFFFFFF);
                self.mach.mov_ri64(reg::RAX, v as i64);
            }
            Expr::Tuple(items, span) => {
                self.mach.line(span.line);
                self.mach.mov_ri32(reg::RDI, (items.len() * 8) as i32);
                self.mach.mov_ri32(reg::RSI, 0);
                self.call_runtime("__alloc_struct");
                let off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, off, reg::RAX);
                for (i, it) in items.iter().enumerate() {
                    self.compile_expr(it);
                    self.mach.mov_rm(reg::RCX, reg::RBP, off);
                    self.mach.mov_mr(reg::RCX, i as i32 * 8, reg::RAX);
                }
                self.mach.mov_rm(reg::RAX, reg::RBP, off);
            }
            Expr::StructLit(name, fields, span) => {
                self.mach.line(span.line);
                let size = self.struct_size(name);
                self.mach.mov_ri32(reg::RDI, size as i32);
                self.mach.mov_ri32(reg::RSI, 0);
                self.call_runtime("__alloc_struct");
                let off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, off, reg::RAX);
                for (fname, val) in fields {
                    self.compile_expr(val);
                    self.mach.mov_rm(reg::RCX, reg::RBP, off);
                    let foff = self.field_offset_for(fname);
                    self.mach.mov_mr(reg::RCX, foff as i32, reg::RAX);
                }
                self.mach.mov_rm(reg::RAX, reg::RBP, off);
            }
            Expr::Match(_, _, _) | Expr::Closure(_, _, _) | Expr::MacroCall(_, _, _) => {
                // QES-SEC-002: never compile an unimplemented construct to a silent 0.
                let span = e.span();
                let what = match e {
                    Expr::Match(..) => "match expression",
                    Expr::Closure(..) => "closure",
                    _ => "macro call",
                };
                self.errors.push(format!(
                    "{}:{}: unsupported {} (not implemented in code generation)",
                    span.line, span.col, what
                ));
                self.mach.mov_ri32(reg::RAX, 0);
            }
        }
    }

    fn compile_binary(&mut self, op: BinOp, a: &Expr, b: &Expr) {
        // For shift ops, count must be in CL.
        match op {
            BinOp::Shl | BinOp::Shr => {
                self.compile_expr(a);
                let a_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, a_off, reg::RAX);
                self.compile_expr(b);
                self.mach.mov_rr(reg::RCX, reg::RAX); // count in CL
                self.mach.mov_rm(reg::RAX, reg::RBP, a_off);
                if op == BinOp::Shl {
                    self.mach.shl_cl(reg::RAX);
                } else {
                    self.mach.sar_cl(reg::RAX);
                }
            }
            _ => {
                self.compile_expr(a);
                let a_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, a_off, reg::RAX);
                self.compile_expr(b);
                self.mach.mov_rr(reg::RCX, reg::RAX);
                self.mach.mov_rm(reg::RAX, reg::RBP, a_off);
                self.emit_binop(op);
            }
        }
    }

    fn emit_binop(&mut self, op: BinOp) {
        match op {
            BinOp::Add => self.mach.add_rr(reg::RAX, reg::RCX),
            BinOp::Sub => self.mach.sub_rr(reg::RAX, reg::RCX),
            BinOp::Mul => self.mach.imul_rr(reg::RAX, reg::RCX),
            BinOp::Div => {
                self.mach.put(0x48); self.mach.put(0x99); // cqo
                self.mach.idiv_r(reg::RCX);
            }
            BinOp::Mod => {
                self.mach.put(0x48); self.mach.put(0x99);
                self.mach.idiv_r(reg::RCX);
                self.mach.mov_rr(reg::RAX, reg::RDX);
            }
            BinOp::BitAnd => self.mach.and_rr(reg::RAX, reg::RCX),
            BinOp::BitOr  => self.mach.or_rr(reg::RAX, reg::RCX),
            BinOp::BitXor => self.mach.xor_rr(reg::RAX, reg::RCX),
            BinOp::Shl => self.mach.shl_cl(reg::RAX),
            BinOp::Shr => self.mach.sar_cl(reg::RAX),
            BinOp::Eq => {
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::Z, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            BinOp::Neq => {
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::NZ, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            BinOp::Lt => {
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::L, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            BinOp::Gt => {
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::NLE, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            BinOp::Le => {
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::LE, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            BinOp::Ge => {
                self.mach.cmp_rr(reg::RAX, reg::RCX);
                self.mach.setcc_r(cc::NL, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
            }
            BinOp::And => {
                self.mach.test_rr(reg::RAX, reg::RAX);
                let jmp_false = self.mach.jcc_rel32(cc::Z);
                self.mach.test_rr(reg::RCX, reg::RCX);
                self.mach.setcc_r(cc::NZ, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
                let end = self.mach.jmp_rel32();
                let fpos = self.mach.pos();
                self.mach.patch_rel32(jmp_false, fpos);
                self.mach.mov_ri32(reg::RAX, 0);
                let epos = self.mach.pos();
                self.mach.patch_rel32(end, epos);
            }
            BinOp::Or => {
                self.mach.test_rr(reg::RAX, reg::RAX);
                let jmp_true = self.mach.jcc_rel32(cc::NZ);
                self.mach.test_rr(reg::RCX, reg::RCX);
                self.mach.setcc_r(cc::NZ, reg::RAX);
                self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
                let end = self.mach.jmp_rel32();
                let tpos = self.mach.pos();
                self.mach.patch_rel32(jmp_true, tpos);
                self.mach.mov_ri32(reg::RAX, 1);
                let epos = self.mach.pos();
                self.mach.patch_rel32(end, epos);
            }
        }
    }

    fn compile_unary(&mut self, op: UnOp, a: &Expr) {
        match op {
            UnOp::AddrOf => {
                // Address-of requires the lvalue's address, not its value.
                match a {
                    Expr::Ident(name, _) => {
                        if let Some(off) = self.lookup_local(name) {
                            self.mach.lea_rm(reg::RAX, reg::RBP, off);
                        } else {
                            self.mach.mov_ri32(reg::RAX, 0);
                        }
                    }
                    Expr::Deref(p, _) => {
                        // &*p == p
                        self.compile_expr(p);
                    }
                    Expr::Field(obj, fname, _) => {
                        self.compile_expr(obj);
                        let off = self.field_offset_for(fname);
                        self.mach.add_ri(reg::RAX, off as i32);
                    }
                    Expr::Index(arr, idx, _) => {
                        self.compile_expr(arr);
                        let arr_off = self.alloc_temp();
                        self.mach.mov_mr(reg::RBP, arr_off, reg::RAX);
                        self.compile_expr(idx);
                        self.mach.mov_rr(reg::RCX, reg::RAX);
                        self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
                        self.mach.lea_rm(reg::RAX, reg::RAX, 16);
                        self.mach.mov_ri32(reg::RDX, 8);
                        self.mach.imul_rr(reg::RDX, reg::RCX);
                        self.mach.add_rr(reg::RAX, reg::RDX);
                    }
                    _ => {
                        self.mach.mov_ri32(reg::RAX, 0);
                    }
                }
            }
            UnOp::Deref => {
                self.compile_expr(a);
                self.mach.mov_rm(reg::RAX, reg::RAX, 0);
            }
            _ => {
                self.compile_expr(a);
                match op {
                    UnOp::Neg => self.mach.neg_r(reg::RAX),
                    UnOp::Not => {
                        self.mach.test_rr(reg::RAX, reg::RAX);
                        self.mach.setcc_r(cc::Z, reg::RAX);
                        self.mach.movzx_r8_r64(reg::RAX, reg::RAX);
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Lvalue helpers
    // -----------------------------------------------------------------------
    fn load_from(&mut self, e: &Expr) {
        self.compile_expr(e);
    }

    fn store_to(&mut self, e: &Expr) {
        match e {
            Expr::Ident(name, _) => {
                if let Some(off) = self.lookup_local(name) {
                    self.mach.mov_mr(reg::RBP, off, reg::RAX);
                }
            }
            Expr::Deref(p, _) | Expr::UnaryOp(UnOp::Deref, p, _) => {
                let val_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, val_off, reg::RAX);
                self.compile_expr(p);
                self.mach.mov_rm(reg::RCX, reg::RBP, val_off);
                self.mach.mov_mr(reg::RAX, 0, reg::RCX);
            }
            Expr::Field(obj, fname, _) => {
                let val_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, val_off, reg::RAX);
                self.compile_expr(obj);
                let off = self.field_offset_for(fname);
                self.mach.mov_rm(reg::RCX, reg::RBP, val_off);
                self.mach.mov_mr(reg::RAX, off as i32, reg::RCX);
            }
            Expr::Index(arr, idx, _) => {
                let val_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, val_off, reg::RAX);
                self.compile_expr(arr);
                let arr_off = self.alloc_temp();
                self.mach.mov_mr(reg::RBP, arr_off, reg::RAX);
                self.compile_expr(idx);
                self.mach.mov_rr(reg::RCX, reg::RAX);
                self.mach.mov_rm(reg::RAX, reg::RBP, arr_off);
                self.mach.lea_rm(reg::RAX, reg::RAX, 16);
                self.mach.mov_ri32(reg::RDX, 8);
                self.mach.imul_rr(reg::RDX, reg::RCX);
                self.mach.add_rr(reg::RAX, reg::RDX);
                self.mach.mov_rm(reg::RCX, reg::RBP, val_off);
                self.mach.mov_mr(reg::RAX, 0, reg::RCX);
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Runtime call helper
    // -----------------------------------------------------------------------
    fn call_runtime(&mut self, name: &str) {
        let p = self.mach.call_rel32();
        self.runtime_patches.push((p, name.to_string()));
    }

    // -----------------------------------------------------------------------
    // Struct / enum metadata
    // -----------------------------------------------------------------------
    fn struct_size(&self, name: &str) -> usize {
        let qname = name.to_string();
        if let Some(s) = self.info.structs.get(&qname) {
            (s.fields.len() * 8).max(8)
        } else {
            8
        }
    }
    fn field_offset_for(&self, name: &str) -> usize {
        for s in self.info.structs.values() {
            if let Some(&off) = s.field_offsets.get(name) {
                return off;
            }
        }
        0
    }
    fn enum_variant_index(&self, enum_name: &str, variant: &str) -> u64 {
        // Try the qualified name first, then fall back to unqualified lookup.
        if let Some(e) = self.info.enums.get(enum_name) {
            for (i, v) in e.variants.iter().enumerate() {
                if v.name == variant {
                    return i as u64;
                }
            }
        }
        // Unqualified lookup: find an enum whose name ends with "::enum_name"
        // or whose name equals enum_name.
        for (qname, e) in &self.info.enums {
            if qname.ends_with(&format!("::{}", enum_name)) || qname == enum_name {
                for (i, v) in e.variants.iter().enumerate() {
                    if v.name == variant {
                        return i as u64;
                    }
                }
            }
        }
        0
    }
}

// Helper: get Linux x86-64 syscall number for a syscall_<name> identifier.
fn syscall_number(name: &str) -> Option<u64> {
    let number = match name {
        "syscall_read" => 0,
        "syscall_write" => 1,
        "syscall_open" => 2,
        "syscall_close" => 3,
        "syscall_stat" => 4,
        "syscall_fstat" => 5,
        "syscall_lstat" => 6,
        "syscall_poll" => 7,
        "syscall_lseek" => 8,
        "syscall_mmap" => 9,
        "syscall_mprotect" => 10,
        "syscall_munmap" => 11,
        "syscall_brk" => 12,
        "syscall_ioctl" => 16,
        "syscall_access" => 21,
        "syscall_pipe" => 22,
        "syscall_select" => 23,
        "syscall_dup" => 32,
        "syscall_dup2" => 33,
        "syscall_fork" => 57,
        "syscall_execve" => 59,
        "syscall_exit" => 60,
        "syscall_wait4" => 61,
        "syscall_kill" => 62,
        "syscall_uname" => 63,
        "syscall_getcwd" => 79,
        "syscall_chdir" => 80,
        "syscall_fchdir" => 81,
        "syscall_mkdir" => 83,
        "syscall_rmdir" => 84,
        "syscall_creat" => 85,
        "syscall_link" => 86,
        "syscall_unlink" => 87,
        "syscall_symlink" => 88,
        "syscall_readlink" => 89,
        "syscall_chmod" => 90,
        "syscall_fchmod" => 91,
        "syscall_chown" => 92,
        "syscall_fchown" => 93,
        "syscall_umask" => 95,
        "syscall_gettimeofday" => 96,
        "syscall_getpid" => 39,
        "syscall_getuid" => 102,
        "syscall_getgid" => 104,
        "syscall_nanosleep" => 35,
        _ => return None,
    };
    Some(number)
}

fn is_runtime_helper(name: &str) -> bool {
    matches!(name,
        "__region_create" | "__region_destroy" | "__alloc_array" | "__alloc_struct" |
        "__poke8" | "__peek8" | "__poke16" | "__peek16" | "__poke32" | "__peek32" |
        "__poke64" | "__peek64" | "__array_push"
    )
}

fn string_hash(s: &str) -> u64 {
    // Normalize: only hash the last path segment (after the last `::`).
    let last = s.rsplit("::").next().unwrap_or(s);
    let mut h: u64 = 14695981039346656037;
    for b in last.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}
