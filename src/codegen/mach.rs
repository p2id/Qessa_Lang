//! Direct x86-64 machine code emitter.
//!
//! All instructions are encoded by hand — no assembler is invoked.
//! Output is a flat `.text` blob; the ELF emitter wraps it.

// ---------------------------------------------------------------------------
// Encoding primitives
// ---------------------------------------------------------------------------

pub struct Mach {
    pub code: Vec<u8>,
    /// Per-instruction source-line annotations, used by the disassembler.
    pub lines: Vec<(usize, usize)>, // (code offset, source line)
}

impl Mach {
    pub fn new() -> Self {
        Self { code: Vec::new(), lines: Vec::new() }
    }

    pub fn pos(&self) -> usize { self.code.len() }

    pub fn put(&mut self, b: u8) {
        self.code.push(b);
    }
    pub fn put_n(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }
    pub fn put_u16(&mut self, v: u16) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    pub fn put_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    pub fn put_u64(&mut self, v: u64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    pub fn put_i8(&mut self, v: i8)  { self.put(v as u8); }
    pub fn put_i16(&mut self, v: i16){ self.put_u16(v as u16); }
    pub fn put_i32(&mut self, v: i32){ self.put_u32(v as u32); }
    pub fn put_i64(&mut self, v: i64){ self.put_u64(v as u64); }

    pub fn line(&mut self, line: usize) {
        self.lines.push((self.pos(), line));
    }

    // -----------------------------------------------------------------------
    // REX prefix
    // -----------------------------------------------------------------------
    fn rex(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let mut v = 0x40u8;
        if w { v |= 0x08; }
        if r & 8 != 0 { v |= 0x04; }
        if x & 8 != 0 { v |= 0x02; }
        if b & 8 != 0 { v |= 0x01; }
        // Always emit REX if W=1 or any high bit is set
        if w || (r|b|x) & 8 != 0 || v != 0x40 {
            self.put(v);
        }
    }

    /// ModR/M byte
    fn modrm(&mut self, mod_: u8, reg: u8, rm: u8) {
        let m = (mod_ & 0x03) << 6;
        let r = (reg & 0x07) << 3;
        let rm = rm & 0x07;
        self.put(m | r | rm);
    }

    /// SIB byte
    fn sib(&mut self, scale: u8, index: u8, base: u8) {
        let s = (scale & 0x03) << 6;
        let i = (index & 0x07) << 3;
        let b = base & 0x07;
        self.put(s | i | b);
    }

    // -----------------------------------------------------------------------
    // Register constants (x86-64 System V calling convention)
    // -----------------------------------------------------------------------
    //  rax=0, rcx=1, rdx=2, rbx=3, rsp=4, rbp=5, rsi=6, rdi=7
    //  r8 =8, r9 =9, r10=10, r11=11, r12=12, r13=13, r14=14, r15=15

    // -----------------------------------------------------------------------
    // Moves
    // -----------------------------------------------------------------------

    /// `mov r64, r64`
    pub fn mov_rr(&mut self, dst: u8, src: u8) {
        // 0x89 = mov r/m64, r64  →  reg=src, rm=dst
        // REX.R extends src, REX.B extends dst.
        self.rex(true, src, 0, dst);
        self.put(0x89);
        self.modrm(0b11, src, dst);
    }

    /// `mov r64, imm64`
    pub fn mov_ri64(&mut self, dst: u8, imm: i64) {
        self.rex(true, 0, 0, dst);
        self.put(0xB8 + (dst & 0x07));
        self.put_i64(imm);
    }

    /// `mov r64, imm32 (sign-extended)`
    pub fn mov_ri32(&mut self, dst: u8, imm: i32) {
        self.rex(true, 0, 0, dst);
        self.put(0xC7);
        self.modrm(0b11, 0, dst);
        self.put_i32(imm);
    }

    /// `mov r64, [r64+disp]`
    pub fn mov_rm(&mut self, dst: u8, base: u8, disp: i32) {
        self.rex(true, dst, 0, base);
        self.put(0x8B);
        self.emit_disp(base, disp, dst);
    }

    /// `mov [r64+disp], r64`
    pub fn mov_mr(&mut self, base: u8, disp: i32, src: u8) {
        self.rex(true, src, 0, base);
        self.put(0x89);
        self.emit_disp(base, disp, src);
    }

    /// `mov r64, [imm64]`  (load from absolute address)
    pub fn mov_rabs(&mut self, dst: u8, abs: i64) {
        self.rex(true, dst, 0, 0);
        self.put(0x8B);
        self.modrm(0b00, dst, 0b101); // [disp32]
        self.put_i32(abs as i32);
    }

    /// `mov [imm64], r64`
    pub fn mov_absr(&mut self, abs: i64, src: u8) {
        self.rex(true, src, 0, 0);
        self.put(0x89);
        self.modrm(0b00, src, 0b101);
        self.put_i32(abs as i32);
    }

    /// Helper: emit modrm + sib + disp for [base+disp] addressing.
    fn emit_disp(&mut self, base: u8, disp: i32, reg: u8) {
        // If base is RBP or R13 we need disp8=0 even for zero disp.
        let no_disp = disp == 0 && (base & 0x07) != 5;
        if no_disp {
            self.modrm(0b00, reg, base);
            if (base & 0x07) == 4 {
                self.sib(0, 4, base); // SIB needed for RSP/R12
            }
        } else if disp >= -128 && disp <= 127 {
            self.modrm(0b01, reg, base);
            if (base & 0x07) == 4 {
                self.sib(0, 4, base);
            }
            self.put_i8(disp as i8);
        } else {
            self.modrm(0b10, reg, base);
            if (base & 0x07) == 4 {
                self.sib(0, 4, base);
            }
            self.put_i32(disp);
        }
    }

    /// `add r64, imm32`
    pub fn add_ri(&mut self, dst: u8, imm: i32) {
        self.rex(true, 0, 0, dst);
        self.put(0x81);
        self.modrm(0b11, 0, dst);
        self.put_i32(imm);
    }
    /// `sub r64, imm32`
    pub fn sub_ri(&mut self, dst: u8, imm: i32) {
        self.rex(true, 0, 0, dst);
        self.put(0x81);
        self.modrm(0b11, 5, dst);
        self.put_i32(imm);
    }

    /// `lea r64, [r64+disp]`
    pub fn lea_rm(&mut self, dst: u8, base: u8, disp: i32) {
        self.rex(true, dst, 0, base);
        self.put(0x8D);
        self.emit_disp(base, disp, dst);
    }

    // -----------------------------------------------------------------------
    // Stack ops
    // -----------------------------------------------------------------------
    pub fn push_r(&mut self, r: u8) {
        if r & 8 != 0 { self.rex(false, 0, 0, r); }
        self.put(0x50 + (r & 0x07));
    }
    pub fn pop_r(&mut self, r: u8) {
        if r & 8 != 0 { self.rex(false, 0, 0, r); }
        self.put(0x58 + (r & 0x07));
    }
    pub fn push_i64(&mut self, v: i64) {
        if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
            self.put(0x68);
            self.put_i32(v as i32);
        } else {
            self.mov_ri64(0, v); // rax
            self.push_r(0);
        }
    }

    // -----------------------------------------------------------------------
    // Arithmetic (on r64)
    // -----------------------------------------------------------------------
    pub fn add_rr(&mut self, dst: u8, src: u8) {
        self.rex(true, src, 0, dst);
        self.put(0x01);
        self.modrm(0b11, src, dst);
    }
    pub fn sub_rr(&mut self, dst: u8, src: u8) {
        self.rex(true, src, 0, dst);
        self.put(0x29);
        self.modrm(0b11, src, dst);
    }
    pub fn imul_rr(&mut self, dst: u8, src: u8) {
        self.rex(true, dst, 0, src);
        self.put(0x0F);
        self.put(0xAF);
        self.modrm(0b11, dst, src);
    }
    /// `idiv r64` (signed)
    pub fn idiv_r(&mut self, src: u8) {
        self.rex(true, 0, 0, src);
        self.put(0xF7);
        self.modrm(0b11, 7, src);
    }
    /// `div r64` (unsigned)
    pub fn div_r(&mut self, src: u8) {
        self.rex(true, 0, 0, src);
        self.put(0xF7);
        self.modrm(0b11, 6, src);
    }
    /// `neg r64`
    pub fn neg_r(&mut self, r: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xF7);
        self.modrm(0b11, 3, r);
    }
    /// `not r64`
    pub fn not_r(&mut self, r: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xF7);
        self.modrm(0b11, 2, r);
    }
    /// `and r64, r64`
    pub fn and_rr(&mut self, dst: u8, src: u8) {
        self.rex(true, src, 0, dst);
        self.put(0x21);
        self.modrm(0b11, src, dst);
    }
    pub fn or_rr(&mut self, dst: u8, src: u8) {
        self.rex(true, src, 0, dst);
        self.put(0x09);
        self.modrm(0b11, src, dst);
    }
    pub fn xor_rr(&mut self, dst: u8, src: u8) {
        self.rex(true, src, 0, dst);
        self.put(0x31);
        self.modrm(0b11, src, dst);
    }
    /// `shl r64, cl`
    pub fn shl_cl(&mut self, r: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xD3);
        self.modrm(0b11, 4, r);
    }
    pub fn shr_cl(&mut self, r: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xD3);
        self.modrm(0b11, 5, r);
    }
    pub fn sar_cl(&mut self, r: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xD3);
        self.modrm(0b11, 7, r);
    }
    pub fn shl_i8(&mut self, r: u8, n: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xC1);
        self.modrm(0b11, 4, r);
        self.put(n);
    }
    pub fn shr_i8(&mut self, r: u8, n: u8) {
        self.rex(true, 0, 0, r);
        self.put(0xC1);
        self.modrm(0b11, 5, r);
        self.put(n);
    }

    /// `cmp r64, r64`
    pub fn cmp_rr(&mut self, a: u8, b: u8) {
        self.rex(true, b, 0, a);
        self.put(0x39);
        self.modrm(0b11, b, a);
    }
    /// `cmp r64, imm32`
    pub fn cmp_ri(&mut self, r: u8, imm: i32) {
        self.rex(true, 0, 0, r);
        self.put(0x81);
        self.modrm(0b11, 7, r);
        self.put_i32(imm);
    }
    /// `test r64, r64`
    pub fn test_rr(&mut self, a: u8, b: u8) {
        self.rex(true, b, 0, a);
        self.put(0x85);
        self.modrm(0b11, b, a);
    }

    // -----------------------------------------------------------------------
    // Control flow
    // -----------------------------------------------------------------------

    /// `jmp rel32` (placeholder: writes 0; caller patches)
    pub fn jmp_rel32(&mut self) -> usize {
        self.put(0xE9);
        let p = self.pos();
        self.put_i32(0);
        p
    }
    pub fn patch_rel32(&mut self, patch_pos: usize, target: usize) {
        let rel = (target as isize) - (patch_pos as isize + 4);
        let bytes = (rel as i32).to_le_bytes();
        self.code[patch_pos..patch_pos + 4].copy_from_slice(&bytes);
    }

    /// `jcc rel32` with condition code (0..15).
    pub fn jcc_rel32(&mut self, cc: u8) -> usize {
        self.put(0x0F);
        self.put(0x80 | (cc & 0x0F));
        let p = self.pos();
        self.put_i32(0);
        p
    }

    /// `call rel32`
    pub fn call_rel32(&mut self) -> usize {
        self.put(0xE8);
        let p = self.pos();
        self.put_i32(0);
        p
    }
    /// `call r64` (indirect)
    pub fn call_r(&mut self, r: u8) {
        self.rex(false, 0, 0, r);
        self.put(0xFF);
        self.modrm(0b11, 2, r);
    }
    pub fn ret(&mut self) { self.put(0xC3); }
    pub fn nop(&mut self) { self.put(0x90); }

    /// `syscall`
    pub fn syscall(&mut self) {
        self.put(0x0F);
        self.put(0x05);
    }
    /// `int 0x80` (legacy)
    pub fn int80(&mut self) {
        self.put(0xCD);
        self.put(0x80);
    }
    /// `ud2` (trap)
    pub fn ud2(&mut self) {
        self.put(0x0F);
        self.put(0x0B);
    }
    /// `hlt`
    pub fn hlt(&mut self) { self.put(0xF4); }

    // -----------------------------------------------------------------------
    // Set byte on condition
    // -----------------------------------------------------------------------
    pub fn setcc_r(&mut self, cc: u8, r: u8) {
        self.rex(false, 0, 0, r);
        self.put(0x0F);
        self.put(0x90 | (cc & 0x0F));
        self.modrm(0b11, 0, r);
    }

    pub fn movzx_r8_r64(&mut self, dst: u8, src: u8) {
        // movzx r64, r8
        self.rex(true, dst, 0, src);
        self.put(0x0F);
        self.put(0xB6);
        self.modrm(0b11, dst, src);
    }

    // -----------------------------------------------------------------------
    // Comment helpers (no-op, just for debugging clarity)
    // -----------------------------------------------------------------------
    pub fn pad_to_align(&mut self, align: usize) {
        while self.code.len() % align != 0 {
            self.put(0x90);
        }
    }
}

// Condition codes for jcc/setcc.
pub mod cc {
    pub const O: u8  = 0x0;
    pub const NO: u8 = 0x1;
    pub const B: u8  = 0x2;
    pub const NB: u8 = 0x3;
    pub const Z: u8  = 0x4;
    pub const NZ: u8 = 0x5;
    pub const BE: u8 = 0x6;
    pub const NBE: u8= 0x7;
    pub const S: u8  = 0x8;
    pub const NS: u8 = 0x9;
    pub const P: u8  = 0xA;
    pub const NP: u8 = 0xB;
    pub const L: u8  = 0xC;
    pub const NL: u8 = 0xD;
    pub const LE: u8 = 0xE;
    pub const NLE: u8= 0xF;
}

// x86-64 register indices (System V AMD64 ABI).
pub mod reg {
    pub const RAX: u8 = 0;
    pub const RCX: u8 = 1;
    pub const RDX: u8 = 2;
    pub const RBX: u8 = 3;
    pub const RSP: u8 = 4;
    pub const RBP: u8 = 5;
    pub const RSI: u8 = 6;
    pub const RDI: u8 = 7;
    pub const R8:  u8 = 8;
    pub const R9:  u8 = 9;
    pub const R10: u8 = 10;
    pub const R11: u8 = 11;
    pub const R12: u8 = 12;
    pub const R13: u8 = 13;
    pub const R14: u8 = 14;
    pub const R15: u8 = 15;
    /// Argument registers (System V) — first 6 integer args.
    pub const ARGS: [u8; 6] = [RDI, RSI, RDX, RCX, R8, R9];
    /// Callee-saved registers.
    pub const CALLEE_SAVED: [u8; 6] = [RBX, RBP, R12, R13, R14, R15];
}
