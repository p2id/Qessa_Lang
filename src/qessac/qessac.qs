// qessac.qs — Self-hosted Qessa compiler
// Written in Qessa itself. Compiles .qs source files to Linux x86-64 ELF executables.
//
// Usage: qessac <input.qs> <output>
//
// This is a minimal subset compiler supporting:
//   - fn name(params) { body }
//   - let name = expr
//   - if / else / while / return
//   - integer literals, string literals
//   - binary ops: + - * / % == != < > <= >= && || & | ^ << >>
//   - unary ops: - ! * &
//   - function calls
//   - syscall_<name>(args)  →  direct Linux x86-64 syscall
//   - __poke8/__peek8/__poke32/__peek32/__poke64/__peek64 runtime helpers
//   - alloc_bytes(n)  →  mmap-based allocation
//
// The compiler reads the source, lexes, parses, generates x86-64 machine code,
// wraps it in a minimal ELF64 executable, and writes it to disk.

// ===========================================================================
// LEXER
// ===========================================================================

// Token kinds (must match parser expectations)
let TOK_INT = 1
let TOK_STR = 2
let TOK_IDENT = 3
let TOK_KW_FN = 4
let TOK_KW_LET = 5
let TOK_KW_IF = 6
let TOK_KW_ELSE = 7
let TOK_KW_WHILE = 8
let TOK_KW_RETURN = 9
let TOK_KW_MUT = 10
let TOK_KW_FOR = 11
let TOK_KW_IN = 12
let TOK_KW_BREAK = 13
let TOK_KW_CONTINUE = 14
let TOK_LPAREN = 15
let TOK_RPAREN = 16
let TOK_LBRACE = 17
let TOK_RBRACE = 18
let TOK_LBRACKET = 19
let TOK_RBRACKET = 20
let TOK_COMMA = 21
let TOK_SEMI = 22
let TOK_COLON = 23
let TOK_ASSIGN = 24
let TOK_PLUS = 25
let TOK_MINUS = 26
let TOK_STAR = 27
let TOK_SLASH = 28
let TOK_PERCENT = 29
let TOK_EQ = 30
let TOK_NEQ = 31
let TOK_LT = 32
let TOK_GT = 33
let TOK_LE = 34
let TOK_GE = 35
let TOK_AND = 36
let TOK_OR = 37
let TOK_NOT = 38
let TOK_AMP = 39
let TOK_PIPE = 40
let TOK_CARET = 41
let TOK_TILDE = 42
let TOK_SHL = 43
let TOK_SHR = 44
let TOK_DOT = 45
let TOK_ARROW = 46
let TOK_NEWLINE = 47
let TOK_EOF = 48
let TOK_CHAR = 49
let TOK_PLUS_EQ = 60
let TOK_MINUS_EQ = 61
let TOK_STAR_EQ = 62
let TOK_SLASH_EQ = 63
let TOK_PERCENT_EQ = 64
let TOK_KW_TRUE = 50
let TOK_KW_FALSE = 51
let TOK_KW_NULL = 52

// Token: [kind, intval, strptr, line, col]  (5 i64s = 40 bytes)
fn tok_make(kind, intval, strptr, line, col) {
    let t = alloc_bytes(40)
    __poke64(t, kind)
    __poke64(t + 8, intval)
    __poke64(t + 16, strptr)
    __poke64(t + 24, line)
    __poke64(t + 32, col)
    return t
}

fn tok_kind(t) { return __peek64(t) }
fn tok_int(t) { return __peek64(t + 8) }
fn tok_str(t) { return __peek64(t + 16) }
fn tok_line(t) { return __peek64(t + 24) }

// Lexer state: [src, pos, len, line, tokens_dynarray]
fn lex_new(src) {
    let l = alloc_bytes(40)
    __poke64(l, src)
    __poke64(l + 8, 0)         // pos
    __poke64(l + 16, strlen(src))  // len
    __poke64(l + 24, 1)        // line
    __poke64(l + 32, dyn_new())  // tokens
    return l
}

fn lex_src(l) { return __peek64(l) }
fn lex_pos(l) { return __peek64(l + 8) }
fn lex_set_pos(l, p) { __poke64(l + 8, p); return l }
fn lex_len(l) { return __peek64(l + 16) }
fn lex_line(l) { return __peek64(l + 24) }
fn lex_set_line(l, n) { __poke64(l + 24, n); return l }
fn lex_tokens(l) { return __peek64(l + 32) }

fn lex_peek(l) {
    let p = lex_pos(l)
    let src = lex_src(l)
    if p >= lex_len(l) {
        return 0
    }
    return __peek8(src + p)
}

fn lex_peek2(l) {
    let p = lex_pos(l)
    let src = lex_src(l)
    if p + 1 >= lex_len(l) {
        return 0
    }
    return __peek8(src + p + 1)
}

fn lex_advance(l) {
    let c = lex_peek(l)
    if c == 10 {
        lex_set_line(l, lex_line(l) + 1)
    }
    lex_set_pos(l, lex_pos(l) + 1)
    return c
}

fn lex_skip_ws(l) {
    while lex_pos(l) < lex_len(l) {
        let c = lex_peek(l)
        if c == 32 || c == 9 || c == 13 || c == 10 {
            lex_advance(l)
        } else {
            if c == 47 && lex_peek2(l) == 47 {
                // line comment
                while lex_pos(l) < lex_len(l) && lex_peek(l) != 10 {
                    lex_advance(l)
                }
            } else {
                break
            }
        }
    }
    return 0
}

fn lex_is_ident_start(c) {
    return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95
}

fn lex_is_ident(c) {
    return lex_is_ident_start(c) || (c >= 48 && c <= 57)
}

fn lex_is_digit(c) {
    return c >= 48 && c <= 57
}

fn lex_read_ident(l) {
    let src = lex_src(l)
    let start = lex_pos(l)
    while lex_pos(l) < lex_len(l) && lex_is_ident(lex_peek(l)) {
        lex_advance(l)
    }
    let len = lex_pos(l) - start
    let s = str_new(len)
    memcpy(s, src + start, len)
    return s
}

fn lex_read_number(l) {
    let src = lex_src(l)
    let start = lex_pos(l)
    // Check for hex prefix 0x
    if lex_peek(l) == 48 && lex_peek2(l) == 120 {
        lex_advance(l)
        lex_advance(l)
        let hex_start = lex_pos(l)
        while lex_pos(l) < lex_len(l) && lex_is_hex_digit(lex_peek(l)) {
            lex_advance(l)
        }
        let len = lex_pos(l) - hex_start
        let tmp = str_new(len)
        memcpy(tmp, src + hex_start, len)
        return atox(tmp)
    }
    while lex_pos(l) < lex_len(l) && lex_is_digit(lex_peek(l)) {
        lex_advance(l)
    }
    let len = lex_pos(l) - start
    // Copy digits to a temp buffer and parse
    let tmp = str_new(len)
    memcpy(tmp, src + start, len)
    let val = atou(tmp)
    return val
}

fn lex_is_hex_digit(c) {
    if c >= 48 && c <= 57 { return 1 }
    if c >= 65 && c <= 70 { return 1 }
    if c >= 97 && c <= 102 { return 1 }
    return 0
}

fn atox(s) {
    let result = 0
    let p = s
    let c = __peek8(p)
    while c != 0 {
        let d = 0
        if c >= 48 && c <= 57 { d = c - 48 }
        if c >= 65 && c <= 70 { d = c - 55 }
        if c >= 97 && c <= 102 { d = c - 87 }
        result = result * 16 + d
        p = p + 1
        c = __peek8(p)
    }
    return result
}

fn lex_read_string(l) {
    // assumes opening " already consumed
    let src = lex_src(l)
    let buf = alloc_bytes(4096)
    let n = 0
    while lex_pos(l) < lex_len(l) {
        let c = lex_peek(l)
        if c == 34 {
            lex_advance(l)
            break
        }
        if c == 92 {
            // escape
            lex_advance(l)
            let e = lex_advance(l)
            if e == 110 {
                __poke8(buf + n, 10)
            } else {
                if e == 116 {
                    __poke8(buf + n, 9)
                } else {
                    if e == 114 {
                        __poke8(buf + n, 13)
                    } else {
                        if e == 48 {
                            __poke8(buf + n, 0)
                        } else {
                            if e == 92 {
                                __poke8(buf + n, 92)
                            } else {
                                if e == 34 {
                                    __poke8(buf + n, 34)
                                } else {
                                    __poke8(buf + n, e)
                                }
                            }
                        }
                    }
                }
            }
            n = n + 1
        } else {
            __poke8(buf + n, c)
            n = n + 1
            lex_advance(l)
        }
    }
    __poke8(buf + n, 0)
    // Copy to a right-sized buffer
    let s = str_new(n)
    memcpy(s, buf, n)
    return s
}

fn lex_keyword(s) {
    if strcmp(s, "fn") == 0 { return TOK_KW_FN }
    if strcmp(s, "let") == 0 { return TOK_KW_LET }
    if strcmp(s, "if") == 0 { return TOK_KW_IF }
    if strcmp(s, "else") == 0 { return TOK_KW_ELSE }
    if strcmp(s, "while") == 0 { return TOK_KW_WHILE }
    if strcmp(s, "return") == 0 { return TOK_KW_RETURN }
    if strcmp(s, "mut") == 0 { return TOK_KW_MUT }
    if strcmp(s, "for") == 0 { return TOK_KW_FOR }
    if strcmp(s, "in") == 0 { return TOK_KW_IN }
    if strcmp(s, "break") == 0 { return TOK_KW_BREAK }
    if strcmp(s, "continue") == 0 { return TOK_KW_CONTINUE }
    if strcmp(s, "true") == 0 { return TOK_KW_TRUE }
    if strcmp(s, "false") == 0 { return TOK_KW_FALSE }
    if strcmp(s, "null") == 0 { return TOK_KW_NULL }
    return 0
}

fn lex_next(l) {
    lex_skip_ws(l)
    let pos = lex_pos(l)
    let len = lex_len(l)
    if pos >= len {
        return tok_make(TOK_EOF, 0, 0, lex_line(l), 0)
    }
    let line = lex_line(l)
    let c = lex_peek(l)

    // Identifier / keyword
    if lex_is_ident_start(c) {
        let s = lex_read_ident(l)
        let kw = lex_keyword(s)
        if kw != 0 {
            return tok_make(kw, 0, s, line, 0)
        }
        return tok_make(TOK_IDENT, 0, s, line, 0)
    }

    // Number
    if lex_is_digit(c) {
        let v = lex_read_number(l)
        return tok_make(TOK_INT, v, 0, line, 0)
    }

    // String
    if c == 34 {
        lex_advance(l)
        let s = lex_read_string(l)
        return tok_make(TOK_STR, 0, s, line, 0)
    }

    // Char
    if c == 39 {
        lex_advance(l)
        let ch = lex_advance(l)
        if ch == 92 {
            let e = lex_advance(l)
            if e == 110 {
                ch = 10
            } else {
                if e == 116 {
                    ch = 9
                } else {
                    if e == 48 {
                        ch = 0
                    } else {
                        if e == 92 {
                            ch = 92
                        } else {
                            if e == 39 {
                                ch = 39
                            } else {
                                ch = e
                            }
                        }
                    }
                }
            }
        }
        // expect closing '
        if lex_peek(l) == 39 {
            lex_advance(l)
        }
        return tok_make(TOK_CHAR, ch, 0, line, 0)
    }

    // Punctuation & operators
    lex_advance(l)
    if c == 40 { return tok_make(TOK_LPAREN, 0, 0, line, 0) }
    if c == 41 { return tok_make(TOK_RPAREN, 0, 0, line, 0) }
    if c == 123 { return tok_make(TOK_LBRACE, 0, 0, line, 0) }
    if c == 125 { return tok_make(TOK_RBRACE, 0, 0, line, 0) }
    if c == 91 { return tok_make(TOK_LBRACKET, 0, 0, line, 0) }
    if c == 93 { return tok_make(TOK_RBRACKET, 0, 0, line, 0) }
    if c == 44 { return tok_make(TOK_COMMA, 0, 0, line, 0) }
    if c == 59 { return tok_make(TOK_SEMI, 0, 0, line, 0) }
    if c == 58 { return tok_make(TOK_COLON, 0, 0, line, 0) }
    if c == 46 { return tok_make(TOK_DOT, 0, 0, line, 0) }
    if c == 126 { return tok_make(TOK_TILDE, 0, 0, line, 0) }
    if c == 94 { return tok_make(TOK_CARET, 0, 0, line, 0) }
    if c == 38 {
        if lex_peek(l) == 38 {
            lex_advance(l)
            return tok_make(TOK_AND, 0, 0, line, 0)
        }
        return tok_make(TOK_AMP, 0, 0, line, 0)
    }
    if c == 124 {
        if lex_peek(l) == 124 {
            lex_advance(l)
            return tok_make(TOK_OR, 0, 0, line, 0)
        }
        return tok_make(TOK_PIPE, 0, 0, line, 0)
    }
    if c == 61 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_EQ, 0, 0, line, 0)
        }
        return tok_make(TOK_ASSIGN, 0, 0, line, 0)
    }
    if c == 33 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_NEQ, 0, 0, line, 0)
        }
        return tok_make(TOK_NOT, 0, 0, line, 0)
    }
    if c == 60 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_LE, 0, 0, line, 0)
        }
        if lex_peek(l) == 60 {
            lex_advance(l)
            return tok_make(TOK_SHL, 0, 0, line, 0)
        }
        return tok_make(TOK_LT, 0, 0, line, 0)
    }
    if c == 62 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_GE, 0, 0, line, 0)
        }
        if lex_peek(l) == 62 {
            lex_advance(l)
            return tok_make(TOK_SHR, 0, 0, line, 0)
        }
        return tok_make(TOK_GT, 0, 0, line, 0)
    }
    if c == 45 {
        if lex_peek(l) == 62 {
            lex_advance(l)
            return tok_make(TOK_ARROW, 0, 0, line, 0)
        }
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_MINUS_EQ, 0, 0, line, 0)
        }
        return tok_make(TOK_MINUS, 0, 0, line, 0)
    }
    if c == 43 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_PLUS_EQ, 0, 0, line, 0)
        }
        return tok_make(TOK_PLUS, 0, 0, line, 0)
    }
    if c == 42 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_STAR_EQ, 0, 0, line, 0)
        }
        return tok_make(TOK_STAR, 0, 0, line, 0)
    }
    if c == 47 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_SLASH_EQ, 0, 0, line, 0)
        }
        return tok_make(TOK_SLASH, 0, 0, line, 0)
    }
    if c == 37 {
        if lex_peek(l) == 61 {
            lex_advance(l)
            return tok_make(TOK_PERCENT_EQ, 0, 0, line, 0)
        }
        return tok_make(TOK_PERCENT, 0, 0, line, 0)
    }

    die("lex: unexpected char")
    return tok_make(TOK_EOF, 0, 0, line, 0)
}

fn lex_all(src) {
    let l = lex_new(src)
    let toks = lex_tokens(l)
    let t = 1
    while t != 0 {
        let tok = lex_next(l)
        dyn_push(toks, tok)
        if tok_kind(tok) == TOK_EOF {
            t = 0
        }
    }
    return toks
}

// ===========================================================================
// PARSER
// ===========================================================================

// AST node kinds
let AST_INT = 1
let AST_STR = 2
let AST_IDENT = 3
let AST_BOOL = 4
let AST_NULL = 5
let AST_CHAR = 6
let AST_BINARY = 7
let AST_UNARY = 8
let AST_ASSIGN = 9
let AST_CALL = 10
let AST_INDEX = 11
let AST_FIELD = 12
let AST_ADDR_OF = 13
let AST_DEREF = 14
let AST_IF = 15
let AST_WHILE = 16
let AST_RETURN = 17
let AST_LET = 18
let AST_EXPR_STMT = 19
let AST_BLOCK = 20
let AST_BREAK = 21
let AST_CONTINUE = 22
let AST_FUNC = 23
let AST_PROGRAM = 24
let AST_FOR = 25
let AST_ARRAY_LIT = 26

// AST node layout (variable size, but base is 32 bytes):
//   [kind, intval, strptr, child1, child2, ...]
// For lists (block, call args), we use a dynarray stored at child1.

fn ast_make(kind) {
    let n = alloc_bytes(80)
    __poke64(n, kind)
    __poke64(n + 8, 0)   // intval
    __poke64(n + 16, 0)  // strptr
    __poke64(n + 24, 0)  // child1
    __poke64(n + 32, 0)  // child2
    __poke64(n + 40, 0)  // child3
    __poke64(n + 48, 0)  // extra1
    __poke64(n + 56, 0)  // extra2
    __poke64(n + 64, 0)  // line
    __poke64(n + 72, 0)  // extra3
    return n
}

fn ast_kind(n) { return __peek64(n) }
fn ast_int(n) { return __peek64(n + 8) }
fn ast_set_int(n, v) { __poke64(n + 8, v); return n }
fn ast_str(n) { return __peek64(n + 16) }
fn ast_set_str(n, s) { __poke64(n + 16, s); return n }
fn ast_c1(n) { return __peek64(n + 24) }
fn ast_set_c1(n, v) { __poke64(n + 24, v); return n }
fn ast_c2(n) { return __peek64(n + 32) }
fn ast_set_c2(n, v) { __poke64(n + 32, v); return n }
fn ast_c3(n) { return __peek64(n + 40) }
fn ast_set_c3(n, v) { __poke64(n + 40, v); return n }
fn ast_e1(n) { return __peek64(n + 48) }
fn ast_set_e1(n, v) { __poke64(n + 48, v); return n }
fn ast_e2(n) { return __peek64(n + 56) }
fn ast_set_e2(n, v) { __poke64(n + 56, v); return n }
fn ast_line(n) { return __peek64(n + 64) }
fn ast_set_line(n, v) { __poke64(n + 64, v); return n }

// Parser state: [tokens_dynarray, pos]
fn parser_new(tokens) {
    let p = alloc_bytes(16)
    __poke64(p, tokens)
    __poke64(p + 8, 0)
    return p
}

fn parser_tokens(p) { return __peek64(p) }
fn parser_pos(p) { return __peek64(p + 8) }
fn parser_set_pos(p, n) { __poke64(p + 8, n); return p }

fn parser_cur(p) {
    let toks = parser_tokens(p)
    return dyn_get(toks, parser_pos(p))
}

fn parser_bump(p) {
    let t = parser_cur(p)
    parser_set_pos(p, parser_pos(p) + 1)
    return t
}

fn parser_at(p, kind) {
    return tok_kind(parser_cur(p)) == kind
}

fn parser_eat(p, kind) {
    if parser_at(p, kind) {
        return parser_bump(p)
    }
    let cur = parser_cur(p)
    print_str("error: parse error at line ")
    print_int(tok_line(cur))
    print_str(": expected token ")
    print_int(kind)
    print_str(" but got ")
    print_int(tok_kind(cur))
    print_str("\n")
    syscall_exit(1)
    return 0
}

fn parser_eat_maybe(p, kind) {
    if parser_at(p, kind) {
        parser_bump(p)
        return 1
    }
    return 0
}

// Skip newlines/semis (statement separators)
fn parser_skip_seps(p) {
    while parser_at(p, TOK_SEMI) || parser_at(p, TOK_NEWLINE) {
        parser_bump(p)
    }
    return 0
}

// --- Expression parsing (Pratt-style precedence) ---

fn parse_expr(p) {
    return parse_assign(p)
}

let AST_COMPOUND = 27

fn parse_assign(p) {
    let lhs = parse_or(p)
    if parser_at(p, TOK_ASSIGN) {
        parser_bump(p)
        let rhs = parse_assign(p)
        let n = ast_make(AST_ASSIGN)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        return n
    }
    // Compound assignment: x += y → x = x + y
    if parser_at(p, TOK_PLUS_EQ) {
        parser_bump(p)
        let rhs = parse_assign(p)
        let binop = ast_make(AST_BINARY)
        ast_set_int(binop, TOK_PLUS)
        ast_set_c1(binop, lhs)
        ast_set_c2(binop, rhs)
        let n = ast_make(AST_ASSIGN)
        ast_set_c1(n, lhs)
        ast_set_c2(n, binop)
        return n
    }
    if parser_at(p, TOK_MINUS_EQ) {
        parser_bump(p)
        let rhs = parse_assign(p)
        let binop = ast_make(AST_BINARY)
        ast_set_int(binop, TOK_MINUS)
        ast_set_c1(binop, lhs)
        ast_set_c2(binop, rhs)
        let n = ast_make(AST_ASSIGN)
        ast_set_c1(n, lhs)
        ast_set_c2(n, binop)
        return n
    }
    if parser_at(p, TOK_STAR_EQ) {
        parser_bump(p)
        let rhs = parse_assign(p)
        let binop = ast_make(AST_BINARY)
        ast_set_int(binop, TOK_STAR)
        ast_set_c1(binop, lhs)
        ast_set_c2(binop, rhs)
        let n = ast_make(AST_ASSIGN)
        ast_set_c1(n, lhs)
        ast_set_c2(n, binop)
        return n
    }
    if parser_at(p, TOK_SLASH_EQ) {
        parser_bump(p)
        let rhs = parse_assign(p)
        let binop = ast_make(AST_BINARY)
        ast_set_int(binop, TOK_SLASH)
        ast_set_c1(binop, lhs)
        ast_set_c2(binop, rhs)
        let n = ast_make(AST_ASSIGN)
        ast_set_c1(n, lhs)
        ast_set_c2(n, binop)
        return n
    }
    if parser_at(p, TOK_PERCENT_EQ) {
        parser_bump(p)
        let rhs = parse_assign(p)
        let binop = ast_make(AST_BINARY)
        ast_set_int(binop, TOK_PERCENT)
        ast_set_c1(binop, lhs)
        ast_set_c2(binop, rhs)
        let n = ast_make(AST_ASSIGN)
        ast_set_c1(n, lhs)
        ast_set_c2(n, binop)
        return n
    }
    return lhs
}

fn parse_or(p) {
    let lhs = parse_and(p)
    while parser_at(p, TOK_OR) {
        parser_bump(p)
        let rhs = parse_and(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, TOK_OR)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_and(p) {
    let lhs = parse_bitor(p)
    while parser_at(p, TOK_AND) {
        parser_bump(p)
        let rhs = parse_bitor(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, TOK_AND)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_bitor(p) {
    let lhs = parse_bitxor(p)
    while parser_at(p, TOK_PIPE) {
        parser_bump(p)
        let rhs = parse_bitxor(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, TOK_PIPE)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_bitxor(p) {
    let lhs = parse_bitand(p)
    while parser_at(p, TOK_CARET) {
        parser_bump(p)
        let rhs = parse_bitand(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, TOK_CARET)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_bitand(p) {
    let lhs = parse_eq(p)
    while parser_at(p, TOK_AMP) {
        parser_bump(p)
        let rhs = parse_eq(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, TOK_AMP)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_eq(p) {
    let lhs = parse_cmp(p)
    while parser_at(p, TOK_EQ) || parser_at(p, TOK_NEQ) {
        let op = tok_kind(parser_bump(p))
        let rhs = parse_cmp(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, op)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_cmp(p) {
    let lhs = parse_shift(p)
    while parser_at(p, TOK_LT) || parser_at(p, TOK_GT) || parser_at(p, TOK_LE) || parser_at(p, TOK_GE) {
        let op = tok_kind(parser_bump(p))
        let rhs = parse_shift(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, op)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_shift(p) {
    let lhs = parse_add(p)
    while parser_at(p, TOK_SHL) || parser_at(p, TOK_SHR) {
        let op = tok_kind(parser_bump(p))
        let rhs = parse_add(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, op)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_add(p) {
    let lhs = parse_mul(p)
    while parser_at(p, TOK_PLUS) || parser_at(p, TOK_MINUS) {
        let op = tok_kind(parser_bump(p))
        let rhs = parse_mul(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, op)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_mul(p) {
    let lhs = parse_unary(p)
    while parser_at(p, TOK_STAR) || parser_at(p, TOK_SLASH) || parser_at(p, TOK_PERCENT) {
        let op = tok_kind(parser_bump(p))
        let rhs = parse_unary(p)
        let n = ast_make(AST_BINARY)
        ast_set_int(n, op)
        ast_set_c1(n, lhs)
        ast_set_c2(n, rhs)
        lhs = n
    }
    return lhs
}

fn parse_unary(p) {
    if parser_at(p, TOK_MINUS) {
        parser_bump(p)
        let e = parse_unary(p)
        let n = ast_make(AST_UNARY)
        ast_set_int(n, TOK_MINUS)
        ast_set_c1(n, e)
        return n
    }
    if parser_at(p, TOK_NOT) {
        parser_bump(p)
        let e = parse_unary(p)
        let n = ast_make(AST_UNARY)
        ast_set_int(n, TOK_NOT)
        ast_set_c1(n, e)
        return n
    }
    if parser_at(p, TOK_AMP) {
        parser_bump(p)
        let e = parse_unary(p)
        let n = ast_make(AST_ADDR_OF)
        ast_set_c1(n, e)
        return n
    }
    if parser_at(p, TOK_STAR) {
        parser_bump(p)
        let e = parse_unary(p)
        let n = ast_make(AST_DEREF)
        ast_set_c1(n, e)
        return n
    }
    return parse_postfix(p)
}

fn parse_postfix(p) {
    let e = parse_primary(p)
    while 1 {
        if parser_at(p, TOK_LPAREN) {
            parser_bump(p)
            let args = dyn_new()
            while !parser_at(p, TOK_RPAREN) {
                dyn_push(args, parse_expr(p))
                if !parser_eat_maybe(p, TOK_COMMA) {
                    break
                }
            }
            parser_eat(p, TOK_RPAREN)
            let n = ast_make(AST_CALL)
            ast_set_c1(n, e)
            ast_set_c2(n, args)
            e = n
        } else {
            if parser_at(p, TOK_LBRACKET) {
                parser_bump(p)
                let idx = parse_expr(p)
                parser_eat(p, TOK_RBRACKET)
                let n = ast_make(AST_INDEX)
                ast_set_c1(n, e)
                ast_set_c2(n, idx)
                e = n
            } else {
                if parser_at(p, TOK_DOT) {
                    parser_bump(p)
                    let name_tok = parser_eat(p, TOK_IDENT)
                    let n = ast_make(AST_FIELD)
                    ast_set_c1(n, e)
                    ast_set_str(n, tok_str(name_tok))
                    e = n
                } else {
                    break
                }
            }
        }
    }
    return e
}

fn parse_primary(p) {
    if parser_at(p, TOK_INT) {
        let t = parser_bump(p)
        let n = ast_make(AST_INT)
        ast_set_int(n, tok_int(t))
        return n
    }
    if parser_at(p, TOK_CHAR) {
        let t = parser_bump(p)
        let n = ast_make(AST_CHAR)
        ast_set_int(n, tok_int(t))
        return n
    }
    if parser_at(p, TOK_STR) {
        let t = parser_bump(p)
        let n = ast_make(AST_STR)
        ast_set_str(n, tok_str(t))
        return n
    }
    if parser_at(p, TOK_KW_TRUE) {
        parser_bump(p)
        let n = ast_make(AST_BOOL)
        ast_set_int(n, 1)
        return n
    }
    if parser_at(p, TOK_KW_FALSE) {
        parser_bump(p)
        let n = ast_make(AST_BOOL)
        ast_set_int(n, 0)
        return n
    }
    if parser_at(p, TOK_KW_NULL) {
        parser_bump(p)
        return ast_make(AST_NULL)
    }
    if parser_at(p, TOK_LBRACKET) {
        parser_bump(p)
        let elems = dyn_new()
        while !parser_at(p, TOK_RBRACKET) && !parser_at(p, TOK_EOF) {
            dyn_push(elems, parse_expr(p))
            if !parser_eat_maybe(p, TOK_COMMA) {
                break
            }
        }
        parser_eat(p, TOK_RBRACKET)
        let n = ast_make(AST_ARRAY_LIT)
        ast_set_c1(n, elems)
        return n
    }
    if parser_at(p, TOK_IDENT) {
        let t = parser_bump(p)
        let n = ast_make(AST_IDENT)
        ast_set_str(n, tok_str(t))
        return n
    }
    if parser_at(p, TOK_LPAREN) {
        parser_bump(p)
        let e = parse_expr(p)
        parser_eat(p, TOK_RPAREN)
        return e
    }
    print_str("error: unexpected token in expression at line ")
    print_int(tok_line(parser_cur(p)))
    print_str("\n")
    syscall_exit(1)
    return ast_make(AST_NULL)
}

// --- Statement parsing ---

fn parse_stmt(p) {
    parser_skip_seps(p)
    if parser_at(p, TOK_KW_LET) {
        parser_bump(p)
        parser_eat_maybe(p, TOK_KW_MUT)
        let name_tok = parser_eat(p, TOK_IDENT)
        let name = tok_str(name_tok)
        let init = 0
        if parser_at(p, TOK_ASSIGN) {
            parser_bump(p)
            init = parse_expr(p)
        }
        parser_eat_maybe(p, TOK_SEMI)
        let n = ast_make(AST_LET)
        ast_set_str(n, name)
        ast_set_c1(n, init)
        return n
    }
    if parser_at(p, TOK_KW_RETURN) {
        parser_bump(p)
        let e = 0
        if !parser_at(p, TOK_SEMI) && !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
            e = parse_expr(p)
        }
        parser_eat_maybe(p, TOK_SEMI)
        let n = ast_make(AST_RETURN)
        ast_set_c1(n, e)
        return n
    }
    if parser_at(p, TOK_KW_IF) {
        parser_bump(p)
        let cond = parse_expr(p)
        parser_eat(p, TOK_LBRACE)
        let then_block = dyn_new()
        while !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
            dyn_push(then_block, parse_stmt(p))
            parser_skip_seps(p)
        }
        parser_eat(p, TOK_RBRACE)
        let else_block = 0
        if parser_at(p, TOK_KW_ELSE) {
            parser_bump(p)
            if parser_at(p, TOK_KW_IF) {
                let else_stmt = parse_stmt(p)
                else_block = dyn_new()
                dyn_push(else_block, else_stmt)
            } else {
                parser_eat(p, TOK_LBRACE)
                else_block = dyn_new()
                while !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
                    dyn_push(else_block, parse_stmt(p))
                    parser_skip_seps(p)
                }
                parser_eat(p, TOK_RBRACE)
            }
        }
        let n = ast_make(AST_IF)
        ast_set_c1(n, cond)
        ast_set_c2(n, then_block)
        ast_set_c3(n, else_block)
        return n
    }
    if parser_at(p, TOK_KW_WHILE) {
        parser_bump(p)
        let cond = parse_expr(p)
        parser_eat(p, TOK_LBRACE)
        let body = dyn_new()
        while !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
            dyn_push(body, parse_stmt(p))
            parser_skip_seps(p)
        }
        parser_eat(p, TOK_RBRACE)
        parser_eat_maybe(p, TOK_SEMI)
        let n = ast_make(AST_WHILE)
        ast_set_c1(n, cond)
        ast_set_c2(n, body)
        return n
    }
    if parser_at(p, TOK_KW_FOR) {
        parser_bump(p)
        let var_tok = parser_eat(p, TOK_IDENT)
        let var_name = tok_str(var_tok)
        parser_eat(p, TOK_KW_IN)
        let iter = parse_expr(p)
        parser_eat(p, TOK_LBRACE)
        let body = dyn_new()
        while !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
            dyn_push(body, parse_stmt(p))
            parser_skip_seps(p)
        }
        parser_eat(p, TOK_RBRACE)
        parser_eat_maybe(p, TOK_SEMI)
        let n = ast_make(AST_FOR)
        ast_set_str(n, var_name)
        ast_set_c1(n, iter)
        ast_set_c2(n, body)
        return n
    }
    if parser_at(p, TOK_KW_BREAK) {
        parser_bump(p)
        parser_eat_maybe(p, TOK_SEMI)
        return ast_make(AST_BREAK)
    }
    if parser_at(p, TOK_KW_CONTINUE) {
        parser_bump(p)
        parser_eat_maybe(p, TOK_SEMI)
        return ast_make(AST_CONTINUE)
    }
    if parser_at(p, TOK_LBRACE) {
        parser_bump(p)
        let body = dyn_new()
        while !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
            dyn_push(body, parse_stmt(p))
            parser_skip_seps(p)
        }
        parser_eat(p, TOK_RBRACE)
        let n = ast_make(AST_BLOCK)
        ast_set_c1(n, body)
        return n
    }
    // Expression statement
    let e = parse_expr(p)
    parser_eat_maybe(p, TOK_SEMI)
    let n = ast_make(AST_EXPR_STMT)
    ast_set_c1(n, e)
    return n
}

fn parse_func(p) {
    parser_eat(p, TOK_KW_FN)
    let name_tok = parser_eat(p, TOK_IDENT)
    let name = tok_str(name_tok)
    parser_eat(p, TOK_LPAREN)
    let params = dyn_new()
    while !parser_at(p, TOK_RPAREN) && !parser_at(p, TOK_EOF) {
        let ptok = parser_eat(p, TOK_IDENT)
        dyn_push(params, tok_str(ptok))
        if !parser_eat_maybe(p, TOK_COMMA) {
            break
        }
    }
    parser_eat(p, TOK_RPAREN)
    // optional -> ret type (skip)
    if parser_at(p, TOK_ARROW) {
        parser_bump(p)
        // skip type name
        if parser_at(p, TOK_IDENT) {
            parser_bump(p)
        }
    }
    parser_eat(p, TOK_LBRACE)
    let body = dyn_new()
    parser_skip_seps(p)
    while !parser_at(p, TOK_RBRACE) && !parser_at(p, TOK_EOF) {
        dyn_push(body, parse_stmt(p))
        parser_skip_seps(p)
    }
    parser_eat(p, TOK_RBRACE)
    let n = ast_make(AST_FUNC)
    ast_set_str(n, name)
    ast_set_c1(n, params)
    ast_set_c2(n, body)
    return n
}

fn parse_program(p) {
    let funcs = dyn_new()
    parser_skip_seps(p)
    while !parser_at(p, TOK_EOF) {
        if parser_at(p, TOK_KW_LET) {
            // Top-level let: parse name = value, skip (treat as no-op for codegen).
            // We store the value in a const table for later lookup.
            parser_bump(p)
            parser_eat_maybe(p, TOK_KW_MUT)
            let name_tok = parser_eat(p, TOK_IDENT)
            let name = tok_str(name_tok)
            let init = 0
            if parser_at(p, TOK_ASSIGN) {
                parser_bump(p)
                init = parse_expr(p)
            }
            parser_eat_maybe(p, TOK_SEMI)
            // Store as a fake function node with kind = AST_LET for codegen to skip
            let n = ast_make(AST_LET)
            ast_set_str(n, name)
            ast_set_c1(n, init)
            dyn_push(funcs, n)
        } else {
            dyn_push(funcs, parse_func(p))
        }
        parser_skip_seps(p)
    }
    let n = ast_make(AST_PROGRAM)
    ast_set_c1(n, funcs)
    return n
}

// ===========================================================================
// CODE GENERATION
// ===========================================================================

// CodeGen state:
//   [0]  text buffer (ByteBuffer)
//   [8]  string literals (DynArray of pointers)
//   [16] func names (DynArray of strings)
//   [24] func offsets (DynArray of i64s into text)
//   [32] call patches (DynArray of [patch_pos, func_name_idx])
//   [40] string patches (DynArray of [patch_pos, string_idx])
//   [48] next_local_off
//   [56] locals strtab (name → stack offset)

fn cg_new() {
    let cg = alloc_bytes(104)
    __poke64(cg, buf_new())        // text
    __poke64(cg + 8, dyn_new())    // strings
    __poke64(cg + 16, dyn_new())   // func_names
    __poke64(cg + 24, dyn_new())   // func_offsets
    __poke64(cg + 32, dyn_new())   // call_patches
    __poke64(cg + 40, dyn_new())   // string_patches
    __poke64(cg + 48, 0)           // next_local_off
    __poke64(cg + 56, dyn_new())   // local_names
    __poke64(cg + 64, dyn_new())   // local_offsets
    __poke64(cg + 72, dyn_new())   // const_names
    __poke64(cg + 80, dyn_new())   // const_values
    __poke64(cg + 88, dyn_new())   // break_patches
    __poke64(cg + 96, dyn_new())   // continue_patches
    return cg
}

fn cg_text(cg) { return __peek64(cg) }
fn cg_strings(cg) { return __peek64(cg + 8) }
fn cg_func_names(cg) { return __peek64(cg + 16) }
fn cg_func_offsets(cg) { return __peek64(cg + 24) }
fn cg_call_patches(cg) { return __peek64(cg + 32) }
fn cg_string_patches(cg) { return __peek64(cg + 40) }
fn cg_next_local(cg) { return __peek64(cg + 48) }
fn cg_set_next_local(cg, v) { __poke64(cg + 48, v); return cg }
fn cg_local_names(cg) { return __peek64(cg + 56) }
fn cg_local_offsets(cg) { return __peek64(cg + 64) }
fn cg_const_names(cg) { return __peek64(cg + 72) }
fn cg_const_values(cg) { return __peek64(cg + 80) }
fn cg_break_patches(cg) { return __peek64(cg + 88) }
fn cg_continue_patches(cg) { return __peek64(cg + 96) }

fn cg_lookup_const(cg, name) {
    let names = cg_const_names(cg)
    let vals = cg_const_values(cg)
    let i = 0
    let n = dyn_len(names)
    while i < n {
        if strcmp(dyn_get(names, i), name) == 0 {
            return dyn_get(vals, i)
        }
        i = i + 1
    }
    return 0
}

// True (1) when `name` is a top-level constant, even if its value is 0.
// cg_lookup_const cannot tell "constant with value 0" from "not found".
fn cg_const_exists(cg, name) {
    let names = cg_const_names(cg)
    let i = 0
    let n = dyn_len(names)
    while i < n {
        if strcmp(dyn_get(names, i), name) == 0 {
            return 1
        }
        i = i + 1
    }
    return 0
}

// QES-SEC-002: an unresolved name is a compile error, never a silent 0.
fn cg_fail_unknown_identifier(name) {
    print_str("error: unknown identifier '")
    print_str(name)
    print_str("'\n")
    syscall_exit(1)
    return 0
}

fn cg_alloc_local(cg, name) {
    let off = cg_next_local(cg) + 8
    cg_set_next_local(cg, off)
    dyn_push(cg_local_names(cg), name)
    dyn_push(cg_local_offsets(cg), off)
    return off
}

fn cg_lookup_local(cg, name) {
    let names = cg_local_names(cg)
    let offs = cg_local_offsets(cg)
    // Names may be rebound in later lexical blocks. Match the Rust backend's
    // HashMap behavior by resolving the most recently allocated binding.
    let i = dyn_len(names)
    while i > 0 {
        i = i - 1
        if strcmp(dyn_get(names, i), name) == 0 {
            return dyn_get(offs, i)
        }
    }
    return 0
}

fn cg_intern_string(cg, s) {
    let strs = cg_strings(cg)
    let i = 0
    while i < dyn_len(strs) {
        if strcmp(dyn_get(strs, i), s) == 0 {
            return i
        }
        i = i + 1
    }
    dyn_push(strs, s)
    return dyn_len(strs) - 1
}

// Register constants
let RAX = 0
let RCX = 1
let RDX = 2
let RBX = 3
let RSP = 4
let RBP = 5
let RSI = 6
let RDI = 7
let R8 = 8
let R9 = 9
let R10 = 10
let R11 = 11
let R12 = 12
let R13 = 13
let R14 = 14
let R15 = 15

// REX prefix
fn cg_rex(cg, w, r, x, b) {
    let v = 0x40
    if w != 0 { v = v | 0x08 }
    if (r & 8) != 0 { v = v | 0x04 }
    if (x & 8) != 0 { v = v | 0x02 }
    if (b & 8) != 0 { v = v | 0x01 }
    if w != 0 || (((r | x | b) & 8) != 0) || v != 0x40 {
        buf_push_byte(cg_text(cg), v)
    }
    return 0
}

fn cg_modrm(cg, mod_, reg, rm) {
    let m = (mod_ & 0x03) << 6
    let r = (reg & 0x07) << 3
    let rr = rm & 0x07
    buf_push_byte(cg_text(cg), m | r | rr)
    return 0
}

fn cg_mov_rr(cg, dst, src) {
    cg_rex(cg, 1, src, 0, dst)
    buf_push_byte(cg_text(cg), 0x89)
    cg_modrm(cg, 3, src, dst)
    return 0
}

fn cg_mov_ri32(cg, dst, imm) {
    cg_rex(cg, 1, 0, 0, dst)
    buf_push_byte(cg_text(cg), 0xC7)
    cg_modrm(cg, 3, 0, dst)
    buf_push_i32(cg_text(cg), imm)
    return 0
}

fn cg_mov_ri64(cg, dst, imm) {
    cg_rex(cg, 1, 0, 0, dst)
    buf_push_byte(cg_text(cg), 0xB8 + (dst & 0x07))
    buf_push_i64(cg_text(cg), imm)
    return 0
}

fn cg_mov_rm(cg, dst, base, disp) {
    // mov dst, [base+disp]
    cg_rex(cg, 1, dst, 0, base)
    buf_push_byte(cg_text(cg), 0x8B)
    cg_emit_disp(cg, base, disp, dst)
    return 0
}

fn cg_mov_mr(cg, base, disp, src) {
    // mov [base+disp], src
    cg_rex(cg, 1, src, 0, base)
    buf_push_byte(cg_text(cg), 0x89)
    cg_emit_disp(cg, base, disp, src)
    return 0
}

fn cg_emit_disp(cg, base, disp, reg) {
    let no_disp = 0
    if disp == 0 && (base & 0x07) != 5 {
        no_disp = 1
    }
    if no_disp != 0 {
        cg_modrm(cg, 0, reg, base)
        if (base & 0x07) == 4 {
            // SIB needed for RSP/R12
            buf_push_byte(cg_text(cg), 0x24)
        }
    } else {
        if disp >= -128 && disp <= 127 {
            cg_modrm(cg, 1, reg, base)
            if (base & 0x07) == 4 {
                buf_push_byte(cg_text(cg), 0x24)
            }
            buf_push_byte(cg_text(cg), disp & 0xFF)
        } else {
            cg_modrm(cg, 2, reg, base)
            if (base & 0x07) == 4 {
                buf_push_byte(cg_text(cg), 0x24)
            }
            buf_push_i32(cg_text(cg), disp)
        }
    }
    return 0
}

fn cg_lea_rm(cg, dst, base, disp) {
    cg_rex(cg, 1, dst, 0, base)
    buf_push_byte(cg_text(cg), 0x8D)
    cg_emit_disp(cg, base, disp, dst)
    return 0
}

fn cg_push_r(cg, r) {
    if (r & 8) != 0 { cg_rex(cg, 0, 0, 0, r) }
    buf_push_byte(cg_text(cg), 0x50 + (r & 0x07))
    return 0
}

fn cg_pop_r(cg, r) {
    if (r & 8) != 0 { cg_rex(cg, 0, 0, 0, r) }
    buf_push_byte(cg_text(cg), 0x58 + (r & 0x07))
    return 0
}

fn cg_add_rr(cg, dst, src) {
    cg_rex(cg, 1, src, 0, dst)
    buf_push_byte(cg_text(cg), 0x01)
    cg_modrm(cg, 3, src, dst)
    return 0
}

fn cg_sub_rr(cg, dst, src) {
    cg_rex(cg, 1, src, 0, dst)
    buf_push_byte(cg_text(cg), 0x29)
    cg_modrm(cg, 3, src, dst)
    return 0
}

fn cg_add_ri(cg, dst, imm) {
    cg_rex(cg, 1, 0, 0, dst)
    buf_push_byte(cg_text(cg), 0x81)
    cg_modrm(cg, 3, 0, dst)
    buf_push_i32(cg_text(cg), imm)
    return 0
}

fn cg_sub_ri(cg, dst, imm) {
    cg_rex(cg, 1, 0, 0, dst)
    buf_push_byte(cg_text(cg), 0x81)
    cg_modrm(cg, 3, 5, dst)
    buf_push_i32(cg_text(cg), imm)
    return 0
}

fn cg_imul_rr(cg, dst, src) {
    cg_rex(cg, 1, dst, 0, src)
    buf_push_byte(cg_text(cg), 0x0F)
    buf_push_byte(cg_text(cg), 0xAF)
    cg_modrm(cg, 3, dst, src)
    return 0
}

fn cg_idiv_r(cg, src) {
    cg_rex(cg, 1, 0, 0, src)
    buf_push_byte(cg_text(cg), 0xF7)
    cg_modrm(cg, 3, 7, src)
    return 0
}

fn cg_neg_r(cg, r) {
    cg_rex(cg, 1, 0, 0, r)
    buf_push_byte(cg_text(cg), 0xF7)
    cg_modrm(cg, 3, 3, r)
    return 0
}

fn cg_and_rr(cg, dst, src) {
    cg_rex(cg, 1, src, 0, dst)
    buf_push_byte(cg_text(cg), 0x21)
    cg_modrm(cg, 3, src, dst)
    return 0
}

fn cg_or_rr(cg, dst, src) {
    cg_rex(cg, 1, src, 0, dst)
    buf_push_byte(cg_text(cg), 0x09)
    cg_modrm(cg, 3, src, dst)
    return 0
}

fn cg_xor_rr(cg, dst, src) {
    cg_rex(cg, 1, src, 0, dst)
    buf_push_byte(cg_text(cg), 0x31)
    cg_modrm(cg, 3, src, dst)
    return 0
}

fn cg_shl_cl(cg, r) {
    cg_rex(cg, 1, 0, 0, r)
    buf_push_byte(cg_text(cg), 0xD3)
    cg_modrm(cg, 3, 4, r)
    return 0
}

fn cg_shr_cl(cg, r) {
    cg_rex(cg, 1, 0, 0, r)
    buf_push_byte(cg_text(cg), 0xD3)
    cg_modrm(cg, 3, 7, r)
    return 0
}

fn cg_cmp_rr(cg, a, b) {
    cg_rex(cg, 1, b, 0, a)
    buf_push_byte(cg_text(cg), 0x39)
    cg_modrm(cg, 3, b, a)
    return 0
}

fn cg_test_rr(cg, a, b) {
    cg_rex(cg, 1, b, 0, a)
    buf_push_byte(cg_text(cg), 0x85)
    cg_modrm(cg, 3, b, a)
    return 0
}

fn cg_setcc(cg, cc, r) {
    cg_rex(cg, 0, 0, 0, r)
    buf_push_byte(cg_text(cg), 0x0F)
    buf_push_byte(cg_text(cg), 0x90 | cc)
    cg_modrm(cg, 3, 0, r)
    return 0
}

fn cg_movzx_r8(cg, dst, src) {
    cg_rex(cg, 1, dst, 0, src)
    buf_push_byte(cg_text(cg), 0x0F)
    buf_push_byte(cg_text(cg), 0xB6)
    cg_modrm(cg, 3, dst, src)
    return 0
}

fn cg_jmp_rel32(cg) {
    buf_push_byte(cg_text(cg), 0xE9)
    let p = buf_len(cg_text(cg))
    buf_push_i32(cg_text(cg), 0)
    return p
}

fn cg_jcc_rel32(cg, cc) {
    buf_push_byte(cg_text(cg), 0x0F)
    buf_push_byte(cg_text(cg), 0x80 | cc)
    let p = buf_len(cg_text(cg))
    buf_push_i32(cg_text(cg), 0)
    return p
}

fn cg_call_rel32(cg) {
    buf_push_byte(cg_text(cg), 0xE8)
    let p = buf_len(cg_text(cg))
    buf_push_i32(cg_text(cg), 0)
    return p
}

fn cg_call_r(cg, r) {
    cg_rex(cg, 0, 0, 0, r)
    buf_push_byte(cg_text(cg), 0xFF)
    cg_modrm(cg, 3, 2, r)
    return 0
}

fn cg_ret(cg) {
    buf_push_byte(cg_text(cg), 0xC3)
    return 0
}

fn cg_syscall(cg) {
    buf_push_byte(cg_text(cg), 0x0F)
    buf_push_byte(cg_text(cg), 0x05)
    return 0
}

fn cg_leave(cg) {
    buf_push_byte(cg_text(cg), 0xC9)
    return 0
}

fn cg_patch_rel32(cg, patch_pos, target) {
    let rel = target - (patch_pos + 4)
    buf_set_u32(cg_text(cg), patch_pos, rel)
    return 0
}

// Condition codes
let CC_Z = 0x4
let CC_NZ = 0x5
let CC_L = 0xC
let CC_LE = 0xE
let CC_NL = 0xD
let CC_NLE = 0xF

// Argument registers (System V)
let ARGS_0 = 7  // RDI
let ARGS_1 = 6  // RSI
let ARGS_2 = 2  // RDX
let ARGS_3 = 1  // RCX
let ARGS_4 = 8  // R8
let ARGS_5 = 9  // R9

fn cg_args(i) {
    if i == 0 { return 7 }
    if i == 1 { return 6 }
    if i == 2 { return 2 }
    if i == 3 { return 1 }
    if i == 4 { return 8 }
    if i == 5 { return 9 }
    return 0
}

// Syscall number lookup
fn syscall_num(name) {
    let r = strcmp(name, "syscall_write")
    if r == 0 { return 1 }
    if strcmp(name, "syscall_read") == 0 { return 0 }
    if strcmp(name, "syscall_open") == 0 { return 2 }
    if strcmp(name, "syscall_close") == 0 { return 3 }
    if strcmp(name, "syscall_mmap") == 0 { return 9 }
    if strcmp(name, "syscall_exit") == 0 { return 60 }
    if strcmp(name, "syscall_getpid") == 0 { return 39 }
    return -1  // unknown: callers must reject (QES-SEC-001)
}

// Runtime helper offsets within the runtime blob (emitted by emit_elf).
// These MUST match the order in emit_elf's runtime blob.
fn runtime_offset(name) {
    if strcmp(name, "__poke8") == 0 { return 30 }
    if strcmp(name, "__peek8") == 0 { return 34 }
    if strcmp(name, "__poke32") == 0 { return 39 }
    if strcmp(name, "__peek32") == 0 { return 42 }
    if strcmp(name, "__poke64") == 0 { return 45 }
    if strcmp(name, "__peek64") == 0 { return 49 }
    if strcmp(name, "__alloc_struct") == 0 { return 53 }
    if strcmp(name, "__alloc_array") == 0 { return 53 }
    return -1
}

// --- Expression compilation (result in RAX) ---

fn cg_compile_expr(cg, e) {
    let k = ast_kind(e)
    if k == AST_INT {
        let v = ast_int(e)
        if v >= -2147483648 && v <= 2147483647 {
            cg_mov_ri32(cg, RAX, v)
        } else {
            cg_mov_ri64(cg, RAX, v)
        }
        return 0
    }
    if k == AST_CHAR {
        cg_mov_ri32(cg, RAX, ast_int(e))
        return 0
    }
    if k == AST_BOOL {
        cg_mov_ri32(cg, RAX, ast_int(e))
        return 0
    }
    if k == AST_NULL {
        cg_mov_ri32(cg, RAX, 0)
        return 0
    }
    if k == AST_STR {
        let idx = cg_intern_string(cg, ast_str(e))
        // lea rax, [rip+disp32]  — patched later
        buf_push_byte(cg_text(cg), 0x48)
        buf_push_byte(cg_text(cg), 0x8D)
        buf_push_byte(cg_text(cg), 0x05)
        let p = buf_len(cg_text(cg))
        buf_push_i32(cg_text(cg), 0)
        let patches = cg_string_patches(cg)
        dyn_push(patches, p)
        dyn_push(patches, idx)
        return 0
    }
    if k == AST_IDENT {
        let name = ast_str(e)
        let off = cg_lookup_local(cg, name)
        if off != 0 {
            cg_mov_rm(cg, RAX, RBP, 0 - off)
        } else {
            // Check if it's a top-level const
            let cval = cg_lookup_const(cg, name)
            if cval != 0 {
                cg_mov_ri32(cg, RAX, cval)
            } else {
                // Check if it's a function name → emit lea
                let names = cg_func_names(cg)
                let i = 0
                let found = -1
                while i < dyn_len(names) {
                    if strcmp(dyn_get(names, i), name) == 0 {
                        found = i
                    }
                    i = i + 1
                }
                if found >= 0 {
                    // lea rax, [rip+disp]  — patched with func offset
                    buf_push_byte(cg_text(cg), 0x48)
                    buf_push_byte(cg_text(cg), 0x8D)
                    buf_push_byte(cg_text(cg), 0x05)
                    let p = buf_len(cg_text(cg))
                    buf_push_i32(cg_text(cg), 0)
                    let patches = cg_call_patches(cg)
                    dyn_push(patches, p)
                    dyn_push(patches, 0 - 1 - found)
                } else {
                    // Zero-valued constants fall through to here; anything else is unknown.
                    if cg_const_exists(cg, name) == 0 {
                        cg_fail_unknown_identifier(name)
                    }
                    cg_mov_ri32(cg, RAX, 0)
                }
            }
        }
        return 0
    }
    if k == AST_BINARY {
        cg_compile_binary(cg, e)
        return 0
    }
    if k == AST_UNARY {
        let op = ast_int(e)
        cg_compile_expr(cg, ast_c1(e))
        if op == TOK_MINUS {
            cg_neg_r(cg, RAX)
        }
        if op == TOK_NOT {
            cg_test_rr(cg, RAX, RAX)
            cg_setcc(cg, CC_Z, RAX)
            cg_movzx_r8(cg, RAX, RAX)
        }
        return 0
    }
    if k == AST_ADDR_OF {
        let inner = ast_c1(e)
        if ast_kind(inner) == AST_IDENT {
            let off = cg_lookup_local(cg, ast_str(inner))
            if off != 0 {
                cg_lea_rm(cg, RAX, RBP, 0 - off)
            } else {
                if cg_const_exists(cg, ast_str(inner)) == 0 {
                    cg_fail_unknown_identifier(ast_str(inner))
                }
                cg_mov_ri32(cg, RAX, 0)
            }
        } else {
            cg_mov_ri32(cg, RAX, 0)
        }
        return 0
    }
    if k == AST_DEREF {
        cg_compile_expr(cg, ast_c1(e))
        cg_mov_rm(cg, RAX, RAX, 0)
        return 0
    }
    if k == AST_ASSIGN {
        cg_compile_expr(cg, ast_c2(e))
        let lhs = ast_c1(e)
        if ast_kind(lhs) == AST_IDENT {
            let off = cg_lookup_local(cg, ast_str(lhs))
            if off != 0 {
                cg_mov_mr(cg, RBP, 0 - off, RAX)
            } else {
                if cg_const_exists(cg, ast_str(lhs)) == 0 {
                    cg_fail_unknown_identifier(ast_str(lhs))
                }
            }
        }
        if ast_kind(lhs) == AST_DEREF {
            let val_off = cg_alloc_local(cg, "__tmp")
            cg_mov_mr(cg, RBP, 0 - val_off, RAX)
            cg_compile_expr(cg, ast_c1(lhs))
            cg_mov_rm(cg, RCX, RBP, 0 - val_off)
            cg_mov_mr(cg, RAX, 0, RCX)
        }
        if ast_kind(lhs) == AST_INDEX {
            // arr[idx] = val
            let val_off = cg_alloc_local(cg, "__tmp")
            cg_mov_mr(cg, RBP, 0 - val_off, RAX)
            // Evaluate arr
            cg_compile_expr(cg, ast_c1(lhs))
            let arr_off = cg_alloc_local(cg, "__arr_store")
            cg_mov_mr(cg, RBP, 0 - arr_off, RAX)
            // Evaluate idx
            cg_compile_expr(cg, ast_c2(lhs))
            cg_mov_rr(cg, RCX, RAX)
            // addr = arr + 16 + idx * 8
            cg_mov_rm(cg, RAX, RBP, 0 - arr_off)
            cg_lea_rm(cg, RAX, RAX, 16)
            cg_mov_ri32(cg, RDX, 8)
            cg_imul_rr(cg, RDX, RCX)
            cg_add_rr(cg, RAX, RDX)
            // [addr] = val
            cg_mov_rm(cg, RCX, RBP, 0 - val_off)
            cg_mov_mr(cg, RAX, 0, RCX)
        }
        return 0
    }
    if k == AST_INDEX {
        // arr[idx] — read element
        cg_compile_expr(cg, ast_c1(e))
        let arr_off = cg_alloc_local(cg, "__arr_idx")
        cg_mov_mr(cg, RBP, 0 - arr_off, RAX)
        cg_compile_expr(cg, ast_c2(e))
        cg_mov_rr(cg, RCX, RAX)
        // addr = arr + 16 + idx * 8
        cg_mov_rm(cg, RAX, RBP, 0 - arr_off)
        cg_lea_rm(cg, RAX, RAX, 16)
        cg_mov_ri32(cg, RDX, 8)
        cg_imul_rr(cg, RDX, RCX)
        cg_add_rr(cg, RAX, RDX)
        // rax = [addr]
        cg_mov_rm(cg, RAX, RAX, 0)
        return 0
    }
    if k == AST_CALL {
        cg_compile_call(cg, e)
        return 0
    }
    if k == AST_ARRAY_LIT {
        let elems = ast_c1(e)
        let n = dyn_len(elems)
        // Allocate array: header (16 bytes) + n*8 data
        // Use array_new from stdlib: array_new(cap)
        // But we can't call stdlib functions from codegen.
        // Instead, emit: alloc_bytes(16 + n*8), then set header.
        cg_mov_ri32(cg, RDI, 16 + n * 8)
        cg_mov_ri32(cg, RSI, 0)
        // Call __alloc_struct (runtime offset 53)
        let p1 = cg_call_rel32(cg)
        let patches = cg_call_patches(cg)
        dyn_push(patches, p1)
        dyn_push(patches, 100000 + 53)
        // Store arr in temp
        let arr_off = cg_alloc_local(cg, "__arrlit")
        cg_mov_mr(cg, RBP, 0 - arr_off, RAX)
        // Set length = n
        cg_mov_ri32(cg, RSI, n)
        cg_mov_rm(cg, RCX, RBP, 0 - arr_off)
        // __poke64(arr, n) at offset 0
        cg_mov_rr(cg, RDI, RCX)
        // Call __poke64 (runtime offset 45)
        let p2 = cg_call_rel32(cg)
        dyn_push(patches, p2)
        dyn_push(patches, 100000 + 45)
        // Set capacity = n
        cg_mov_ri32(cg, RSI, n)
        cg_mov_rm(cg, RCX, RBP, 0 - arr_off)
        cg_add_ri(cg, RCX, 8)
        cg_mov_rr(cg, RDI, RCX)
        let p3 = cg_call_rel32(cg)
        dyn_push(patches, p3)
        dyn_push(patches, 100000 + 45)
        // Fill elements
        let i = 0
        while i < n {
            cg_compile_expr(cg, dyn_get(elems, i))
            let elem_off = cg_alloc_local(cg, "__elem")
            cg_mov_mr(cg, RBP, 0 - elem_off, RAX)
            // __poke64(arr + 16 + i*8, elem)
            cg_mov_rm(cg, RCX, RBP, 0 - arr_off)
            cg_add_ri(cg, RCX, 16 + i * 8)
            cg_mov_rr(cg, RDI, RCX)
            cg_mov_rm(cg, RSI, RBP, 0 - elem_off)
            let p4 = cg_call_rel32(cg)
            dyn_push(patches, p4)
            dyn_push(patches, 100000 + 45)
            i = i + 1
        }
        // Return arr
        cg_mov_rm(cg, RAX, RBP, 0 - arr_off)
        return 0
    }
    die("codegen: unknown expr kind")
    return 0
}

fn cg_compile_binary(cg, e) {
    let op = ast_int(e)
    // For shifts, count must be in CL
    if op == TOK_SHL || op == TOK_SHR {
        cg_compile_expr(cg, ast_c1(e))
        let a_off = cg_alloc_local(cg, "__tmp")
        cg_mov_mr(cg, RBP, 0 - a_off, RAX)
        cg_compile_expr(cg, ast_c2(e))
        cg_mov_rr(cg, RCX, RAX)
        cg_mov_rm(cg, RAX, RBP, 0 - a_off)
        if op == TOK_SHL {
            cg_shl_cl(cg, RAX)
        } else {
            cg_shr_cl(cg, RAX)
        }
        return 0
    }
    // Standard binary op
    cg_compile_expr(cg, ast_c1(e))
    let a_off = cg_alloc_local(cg, "__tmp")
    cg_mov_mr(cg, RBP, 0 - a_off, RAX)
    cg_compile_expr(cg, ast_c2(e))
    cg_mov_rr(cg, RCX, RAX)
    cg_mov_rm(cg, RAX, RBP, 0 - a_off)
    if op == TOK_PLUS { cg_add_rr(cg, RAX, RCX); return 0 }
    if op == TOK_MINUS { cg_sub_rr(cg, RAX, RCX); return 0 }
    if op == TOK_STAR { cg_imul_rr(cg, RAX, RCX); return 0 }
    if op == TOK_SLASH {
        // cqo (sign-extend RAX→RDX:RAX)
        buf_push_byte(cg_text(cg), 0x48)
        buf_push_byte(cg_text(cg), 0x99)
        cg_idiv_r(cg, RCX)
        return 0
    }
    if op == TOK_PERCENT {
        buf_push_byte(cg_text(cg), 0x48)
        buf_push_byte(cg_text(cg), 0x99)
        cg_idiv_r(cg, RCX)
        cg_mov_rr(cg, RAX, RDX)
        return 0
    }
    if op == TOK_AMP { cg_and_rr(cg, RAX, RCX); return 0 }
    if op == TOK_PIPE { cg_or_rr(cg, RAX, RCX); return 0 }
    if op == TOK_CARET { cg_xor_rr(cg, RAX, RCX); return 0 }
    if op == TOK_EQ {
        cg_cmp_rr(cg, RAX, RCX)
        cg_setcc(cg, CC_Z, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        return 0
    }
    if op == TOK_NEQ {
        cg_cmp_rr(cg, RAX, RCX)
        cg_setcc(cg, CC_NZ, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        return 0
    }
    if op == TOK_LT {
        cg_cmp_rr(cg, RAX, RCX)
        cg_setcc(cg, CC_L, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        return 0
    }
    if op == TOK_GT {
        cg_cmp_rr(cg, RAX, RCX)
        cg_setcc(cg, CC_NLE, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        return 0
    }
    if op == TOK_LE {
        cg_cmp_rr(cg, RAX, RCX)
        cg_setcc(cg, CC_LE, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        return 0
    }
    if op == TOK_GE {
        cg_cmp_rr(cg, RAX, RCX)
        cg_setcc(cg, CC_NL, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        return 0
    }
    if op == TOK_AND {
        cg_test_rr(cg, RAX, RAX)
        let jf = cg_jcc_rel32(cg, CC_Z)
        cg_test_rr(cg, RCX, RCX)
        cg_setcc(cg, CC_NZ, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        let je = cg_jmp_rel32(cg)
        cg_patch_rel32(cg, jf, buf_len(cg_text(cg)))
        cg_mov_ri32(cg, RAX, 0)
        cg_patch_rel32(cg, je, buf_len(cg_text(cg)))
        return 0
    }
    if op == TOK_OR {
        cg_test_rr(cg, RAX, RAX)
        let jt = cg_jcc_rel32(cg, CC_NZ)
        cg_test_rr(cg, RCX, RCX)
        cg_setcc(cg, CC_NZ, RAX)
        cg_movzx_r8(cg, RAX, RAX)
        let je = cg_jmp_rel32(cg)
        cg_patch_rel32(cg, jt, buf_len(cg_text(cg)))
        cg_mov_ri32(cg, RAX, 1)
        cg_patch_rel32(cg, je, buf_len(cg_text(cg)))
        return 0
    }
    die("codegen: unknown binary op")
    return 0
}

fn cg_compile_call(cg, e) {
    let callee = ast_c1(e)
    let args = ast_c2(e)
    // Evaluate args into temps
    let arg_offs = dyn_new()
    let i = 0
    while i < dyn_len(args) {
        cg_compile_expr(cg, dyn_get(args, i))
        let off = cg_alloc_local(cg, "__arg")
        cg_mov_mr(cg, RBP, 0 - off, RAX)
        dyn_push(arg_offs, off)
        i = i + 1
    }
    // Load args into registers
    i = 0
    while i < dyn_len(arg_offs) && i < 6 {
        cg_mov_rm(cg, cg_args(i), RBP, 0 - dyn_get(arg_offs, i))
        i = i + 1
    }
    // SysV x86-64 stack arguments are laid out left-to-right from low to high
    // addresses, so push arguments beyond the sixth in reverse order.
    let stack_args = 0
    if dyn_len(arg_offs) > 6 {
        stack_args = dyn_len(arg_offs) - 6
    }
    let stack_padding = 0
    if stack_args % 2 != 0 {
        stack_padding = 8
        cg_sub_ri(cg, RSP, stack_padding)
    }
    i = dyn_len(arg_offs)
    while i > 6 {
        i = i - 1
        cg_mov_rm(cg, RAX, RBP, 0 - dyn_get(arg_offs, i))
        cg_push_r(cg, RAX)
    }
    // If callee is an identifier and starts with "syscall_", emit syscall.
    if ast_kind(callee) == AST_IDENT {
        let name = ast_str(callee)
        let prefix_match = strncmp_prefix(name, "syscall_")
        if prefix_match == 0 {
            // Inline syscall number lookup (avoid function call overhead)
            let num = -1  // unknown until matched (QES-SEC-001: never default to exit)
            if strcmp(name, "syscall_write") == 0 { num = 1 }
            if strcmp(name, "syscall_read") == 0 { num = 0 }
            if strcmp(name, "syscall_open") == 0 { num = 2 }
            if strcmp(name, "syscall_close") == 0 { num = 3 }
            if strcmp(name, "syscall_mmap") == 0 { num = 9 }
            if strcmp(name, "syscall_exit") == 0 { num = 60 }
            if strcmp(name, "syscall_getpid") == 0 { num = 39 }
            if num < 0 {
                print_str("error: unknown syscall '")
                print_str(name)
                print_str("'\n")
                syscall_exit(1)
            }
            // syscall_: emit direct Linux syscall
            if dyn_len(arg_offs) >= 4 {
                cg_mov_rr(cg, R10, RCX)
            }
            cg_mov_ri32(cg, RAX, num)
            cg_syscall(cg)
        } else {
            // Search user functions FIRST (most common case)
            let names = cg_func_names(cg)
            let found = -1
            i = 0
            while i < dyn_len(names) {
                if strcmp(dyn_get(names, i), name) == 0 {
                    found = i
                }
                i = i + 1
            }
            if found >= 0 {
                let p = cg_call_rel32(cg)
                let patches = cg_call_patches(cg)
                dyn_push(patches, p)
                dyn_push(patches, found)
            } else {
                // Not found in user functions — only check runtime helpers
                // if name starts with "__" (safety check)
                let c0 = __peek8(name)
                let c1 = __peek8(name + 1)
                if c0 == 95 && c1 == 95 {
                    let rt_off = runtime_offset(name)
                    if rt_off >= 0 {
                        let p = cg_call_rel32(cg)
                        let patches = cg_call_patches(cg)
                        dyn_push(patches, p)
                        dyn_push(patches, 100000 + rt_off)
                    } else {
                        print_str("error: unknown runtime helper '")
                        print_str(name)
                        print_str("'\n")
                        syscall_exit(1)
                    }
                } else {
                    // Unknown function: reject at compile time (QES-SEC-002)
                    print_str("error: unknown function '")
                    print_str(name)
                    print_str("'\n")
                    syscall_exit(1)
                }
            }
        }
    } else {
        // Indirect call
        cg_compile_expr(cg, callee)
        cg_call_r(cg, RAX)
    }
    if stack_args > 0 {
        cg_add_ri(cg, RSP, stack_args * 8 + stack_padding)
    }
    return 0
}

// Check if string s starts with prefix p
fn strncmp_prefix(s, p) {
    let i = 0
    while __peek8(p + i) != 0 {
        let sc = __peek8(s + i)
        let pc = __peek8(p + i)
        if sc != pc {
            return 1
        }
        i = i + 1
    }
    return 0
}

// --- Statement compilation ---

fn cg_compile_stmt(cg, s) {
    let k = ast_kind(s)
    if k == AST_LET {
        let off = cg_alloc_local(cg, ast_str(s))
        let init = ast_c1(s)
        if init != 0 {
            cg_compile_expr(cg, init)
            cg_mov_mr(cg, RBP, 0 - off, RAX)
        } else {
            cg_mov_ri32(cg, RAX, 0)
            cg_mov_mr(cg, RBP, 0 - off, RAX)
        }
        return 0
    }
    if k == AST_EXPR_STMT {
        cg_compile_expr(cg, ast_c1(s))
        return 0
    }
    if k == AST_RETURN {
        let e = ast_c1(s)
        if e != 0 {
            cg_compile_expr(cg, e)
        } else {
            cg_mov_ri32(cg, RAX, 0)
        }
        cg_leave(cg)
        cg_ret(cg)
        return 0
    }
    if k == AST_IF {
        cg_compile_expr(cg, ast_c1(s))
        cg_test_rr(cg, RAX, RAX)
        let jelse = cg_jcc_rel32(cg, CC_Z)
        cg_compile_block(cg, ast_c2(s))
        let jend = cg_jmp_rel32(cg)
        cg_patch_rel32(cg, jelse, buf_len(cg_text(cg)))
        let eb = ast_c3(s)
        if eb != 0 {
            cg_compile_block(cg, eb)
        }
        cg_patch_rel32(cg, jend, buf_len(cg_text(cg)))
        return 0
    }
    if k == AST_WHILE {
        let start = buf_len(cg_text(cg))
        // Push loop context markers
        dyn_push(cg_break_patches(cg), 0 - 1)
        dyn_push(cg_continue_patches(cg), 0 - 1)
        cg_compile_expr(cg, ast_c1(s))
        cg_test_rr(cg, RAX, RAX)
        let jend = cg_jcc_rel32(cg, CC_Z)
        cg_compile_block(cg, ast_c2(s))
        let back = cg_jmp_rel32(cg)
        cg_patch_rel32(cg, back, start)
        let loop_end = buf_len(cg_text(cg))
        cg_patch_rel32(cg, jend, loop_end)
        // Patch all break patches since the marker
        let bp = cg_break_patches(cg)
        while dyn_len(bp) > 0 {
            let v = dyn_get(bp, dyn_len(bp) - 1)
            // Remove last element by decrementing length
            __poke64(bp + 8, dyn_len(bp) - 1)
            if v == 0 - 1 { break }
            cg_patch_rel32(cg, v, loop_end)
        }
        // Patch all continue patches since the marker
        let cp = cg_continue_patches(cg)
        while dyn_len(cp) > 0 {
            let v = dyn_get(cp, dyn_len(cp) - 1)
            __poke64(cp + 8, dyn_len(cp) - 1)
            if v == 0 - 1 { break }
            cg_patch_rel32(cg, v, start)
        }
        return 0
    }
    if k == AST_FOR {
        // for var in array { body }
        // Lower to: let __arr = array; let __n = arr[0]; let __i = 0;
        //   while __i < __n { var = arr[2+__i]; body; __i++ }
        let arr_off = cg_alloc_local(cg, "__for_arr")
        let n_off = cg_alloc_local(cg, "__for_n")
        let i_off = cg_alloc_local(cg, "__for_i")
        let var_off = cg_alloc_local(cg, ast_str(s))

        // Evaluate iter expression, store in __for_arr
        cg_compile_expr(cg, ast_c1(s))
        cg_mov_mr(cg, RBP, 0 - arr_off, RAX)
        // Load length: RAX = [arr] (first 8 bytes = length)
        cg_mov_rm(cg, RAX, RBP, 0 - arr_off)
        cg_mov_rm(cg, RAX, RAX, 0)
        cg_mov_mr(cg, RBP, 0 - n_off, RAX)
        // __i = 0
        cg_mov_ri32(cg, RAX, 0)
        cg_mov_mr(cg, RBP, 0 - i_off, RAX)

        // Loop start
        let start = buf_len(cg_text(cg))
        // Push loop context
        dyn_push(cg_break_patches(cg), 0 - 1)
        dyn_push(cg_continue_patches(cg), 0 - 1)
        // while __i < __n
        cg_mov_rm(cg, RAX, RBP, 0 - i_off)
        cg_mov_rm(cg, RCX, RBP, 0 - n_off)
        cg_cmp_rr(cg, RAX, RCX)
        let jend = cg_jcc_rel32(cg, CC_NL)

        // var = arr[16 + __i * 8]
        cg_mov_rm(cg, RAX, RBP, 0 - arr_off)
        cg_mov_rm(cg, RCX, RBP, 0 - i_off)
        cg_lea_rm(cg, RAX, RAX, 16)
        cg_mov_ri32(cg, RDX, 8)
        cg_imul_rr(cg, RDX, RCX)
        cg_add_rr(cg, RAX, RDX)
        cg_mov_rm(cg, RAX, RAX, 0)
        cg_mov_mr(cg, RBP, 0 - var_off, RAX)

        // Body
        cg_compile_block(cg, ast_c2(s))

        // __i++
        cg_mov_rm(cg, RAX, RBP, 0 - i_off)
        cg_add_ri(cg, RAX, 1)
        cg_mov_mr(cg, RBP, 0 - i_off, RAX)

        // Jump back to start
        let back = cg_jmp_rel32(cg)
        cg_patch_rel32(cg, back, start)
        let loop_end = buf_len(cg_text(cg))
        cg_patch_rel32(cg, jend, loop_end)

        // Patch breaks
        let bp = cg_break_patches(cg)
        while dyn_len(bp) > 0 {
            let v = dyn_get(bp, dyn_len(bp) - 1)
            __poke64(bp + 8, dyn_len(bp) - 1)
            if v == 0 - 1 { break }
            cg_patch_rel32(cg, v, loop_end)
        }
        // Patch continues
        let cp = cg_continue_patches(cg)
        while dyn_len(cp) > 0 {
            let v = dyn_get(cp, dyn_len(cp) - 1)
            __poke64(cp + 8, dyn_len(cp) - 1)
            if v == 0 - 1 { break }
            cg_patch_rel32(cg, v, start)
        }
        return 0
    }
    if k == AST_BLOCK {
        cg_compile_block(cg, ast_c1(s))
        return 0
    }
    if k == AST_BREAK {
        let p = cg_jmp_rel32(cg)
        dyn_push(cg_break_patches(cg), p)
        return 0
    }
    if k == AST_CONTINUE {
        let p = cg_jmp_rel32(cg)
        dyn_push(cg_continue_patches(cg), p)
        return 0
    }
    die("codegen: unknown stmt kind")
    return 0
}

fn cg_compile_block(cg, stmts) {
    let i = 0
    while i < dyn_len(stmts) {
        cg_compile_stmt(cg, dyn_get(stmts, i))
        i = i + 1
    }
    return 0
}

// --- Function compilation ---

fn cg_compile_func(cg, func) {
    // Reset locals
    __poke64(cg + 56, dyn_new())
    __poke64(cg + 64, dyn_new())
    cg_set_next_local(cg, 0)

    let name = ast_str(func)
    let params = ast_c1(func)
    let body = ast_c2(func)

    // Record function offset
    let off = buf_len(cg_text(cg))
    dyn_push(cg_func_offsets(cg), off)

    // Prologue: push rbp; mov rbp, rsp; sub rsp, <placeholder>
    cg_push_r(cg, RBP)
    cg_mov_rr(cg, RBP, RSP)
    buf_push_byte(cg_text(cg), 0x48)
    buf_push_byte(cg_text(cg), 0x81)
    buf_push_byte(cg_text(cg), 0xEC)
    let local_size_patch = buf_len(cg_text(cg))
    buf_push_i32(cg_text(cg), 0)

    // Store params into locals
    let i = 0
    while i < dyn_len(params) {
        let pname = dyn_get(params, i)
        let off2 = cg_alloc_local(cg, pname)
        if i < 6 {
            cg_mov_mr(cg, RBP, 0 - off2, cg_args(i))
        } else {
            // On function entry, the seventh argument is at [rbp + 16].
            let stack_off = 16 + (i - 6) * 8
            cg_mov_rm(cg, RAX, RBP, stack_off)
            cg_mov_mr(cg, RBP, 0 - off2, RAX)
        }
        i = i + 1
    }

    // Compile body
    cg_compile_block(cg, body)

    // Implicit return 0
    cg_mov_ri32(cg, RAX, 0)
    cg_leave(cg)
    cg_ret(cg)

    // Patch local size (16-byte aligned, with extra 256 bytes for temporaries)
    let total = cg_next_local(cg) + 256
    let aligned = (total + 15) & (0 - 16)
    buf_set_u32(cg_text(cg), local_size_patch, aligned)

    return 0
}

// --- Program compilation ---

fn cg_compile_program(cg, prog) {
    let funcs = ast_c1(prog)
    // First pass: register function names and top-level consts
    let i = 0
    while i < dyn_len(funcs) {
        let f = dyn_get(funcs, i)
        if ast_kind(f) == AST_FUNC {
            dyn_push(cg_func_names(cg), ast_str(f))
        }
        if ast_kind(f) == AST_LET {
            let init = ast_c1(f)
            if init != 0 {
                if ast_kind(init) == AST_INT {
                    dyn_push(cg_const_names(cg), ast_str(f))
                    dyn_push(cg_const_values(cg), ast_int(init))
                }
            }
        }
        i = i + 1
    }
    // Second pass: compile each function (skip AST_LET)
    i = 0
    while i < dyn_len(funcs) {
        let f = dyn_get(funcs, i)
        if ast_kind(f) == AST_FUNC {
            cg_compile_func(cg, f)
        }
        i = i + 1
    }
    return 0
}

// ===========================================================================
// ELF EMITTER
// ===========================================================================

fn align_up(v, a) {
    let r = v & (a - 1)
    if r == 0 { return v }
    return v + (a - r)
}

fn emit_elf(cg, output_path) {
    let text = cg_text(cg)
    let text_data = buf_data(text)
    let text_len = buf_len(text)

    // Build runtime stub blob (hand-encoded)
    let runtime = alloc_bytes(4096)
    let rpos = 0

    // _start: load argc and argv from stack, align RSP, call main(argc, argv), exit.
    // At entry: [RSP]=argc, [RSP+8]=argv[0], [RSP+16]=argv[1]...
    // Encoding:
    //   48 8B 7C 24 04        mov rdi, [rsp+4]... wait, argc is at [rsp] (8 bytes).
    // Actually at _start entry: RSP points to argc (8 bytes), then argv array.
    //   48 8B 3C 24           mov rdi, [rsp]      ; rdi = argc       (4 bytes) [0..3]
    //   48 8B 74 24 08        mov rsi, [rsp+8]    ; rsi = argv       (5 bytes) [4..8]
    //   48 83 EC 08           sub rsp, 8          ; align to 16     (4 bytes) [9..12]
    //   E8 xx xx xx xx        call rel32          ; call main       (5 bytes) [13..17]
    //   48 89 C7              mov rdi, rax        ; exit code       (3 bytes) [18..20]
    //   48 C7 C0 E7 00 00 00  mov rax, 231        ; exit_group      (7 bytes) [21..27]
    //   0F 05                 syscall             (2 bytes) [28..29]
    // Total: 30 bytes
    __poke8(runtime + 0, 0x48)  // mov rdi, [rsp]
    __poke8(runtime + 1, 0x8B)
    __poke8(runtime + 2, 0x3C)
    __poke8(runtime + 3, 0x24)
    __poke8(runtime + 4, 0x48)  // lea rsi, [rsp+8]
    __poke8(runtime + 5, 0x8D)
    __poke8(runtime + 6, 0x74)
    __poke8(runtime + 7, 0x24)
    __poke8(runtime + 8, 0x08)
    __poke8(runtime + 9, 0x48)  // sub rsp, 8
    __poke8(runtime + 10, 0x83)
    __poke8(runtime + 11, 0xEC)
    __poke8(runtime + 12, 0x08)
    __poke8(runtime + 13, 0xE8)  // call rel32
    let call_main_patch = 14
    __poke32(runtime + 14, 0)
    __poke8(runtime + 18, 0x48)  // mov rdi, rax
    __poke8(runtime + 19, 0x89)
    __poke8(runtime + 20, 0xC7)
    __poke8(runtime + 21, 0x48)  // mov rax, 231 (exit_group)
    __poke8(runtime + 22, 0xC7)
    __poke8(runtime + 23, 0xC0)
    __poke8(runtime + 24, 231)
    __poke8(runtime + 25, 0)
    __poke8(runtime + 26, 0)
    __poke8(runtime + 27, 0)
    __poke8(runtime + 28, 0x0F)  // syscall
    __poke8(runtime + 29, 0x05)
    // _start ends at offset 30.

    // __poke8(addr, val): mov [rdi], sil; ret  →  40 88 37 C3  (4 bytes)
    __poke8(runtime + 30, 0x40)
    __poke8(runtime + 31, 0x88)
    __poke8(runtime + 32, 0x37)
    __poke8(runtime + 33, 0xC3)

    // __peek8(addr): movzx rax, byte [rdi]; ret  →  48 0F B6 07 C3  (5 bytes)
    __poke8(runtime + 34, 0x48)
    __poke8(runtime + 35, 0x0F)
    __poke8(runtime + 36, 0xB6)
    __poke8(runtime + 37, 0x07)
    __poke8(runtime + 38, 0xC3)

    // __poke32(addr, val): mov [rdi], esi; ret  →  89 37 C3  (3 bytes)
    __poke8(runtime + 39, 0x89)
    __poke8(runtime + 40, 0x37)
    __poke8(runtime + 41, 0xC3)

    // __peek32(addr): mov eax, [rdi]; ret  →  8B 07 C3  (3 bytes)
    __poke8(runtime + 42, 0x8B)
    __poke8(runtime + 43, 0x07)
    __poke8(runtime + 44, 0xC3)

    // __poke64(addr, val): mov [rdi], rsi; ret  →  48 89 37 C3  (4 bytes)
    __poke8(runtime + 45, 0x48)
    __poke8(runtime + 46, 0x89)
    __poke8(runtime + 47, 0x37)
    __poke8(runtime + 48, 0xC3)

    // __peek64(addr): mov rax, [rdi]; ret  →  48 8B 07 C3  (4 bytes)
    __poke8(runtime + 49, 0x48)
    __poke8(runtime + 50, 0x8B)
    __poke8(runtime + 51, 0x07)
    __poke8(runtime + 52, 0xC3)

    // __alloc_struct(size, region): mmap(0, size, 3, 34, -1, 0); ret
    // rdi = size, rsi = region (ignored — always mmap)
    // Use RSI (size from caller) as mmap length
    // 48 89 FE          mov rsi, rdi        ; rsi = size (mmap length)
    // 48 C7 C7 00 00 00 00  mov rdi, 0     ; addr = NULL
    // 48 C7 C2 03 00 00 00  mov rdx, 3     ; PROT_READ|PROT_WRITE
    // 49 C7 C2 22 00 00 00  mov r10, 0x22  ; MAP_PRIVATE|MAP_ANONYMOUS
    // 49 C7 C0 FF FF FF FF  mov r8, -1    ; fd = -1
    // 49 C7 C1 00 00 00 00  mov r9, 0      ; offset = 0
    // 48 C7 C0 09 00 00 00  mov rax, 9    ; SYS_mmap
    // 0F 05             syscall
    // C3                ret
    // Total: 3+7+7+7+7+7+7+2+1 = 48 bytes
    __poke8(runtime + 53, 0x48)  // mov rsi, rdi
    __poke8(runtime + 54, 0x89)
    __poke8(runtime + 55, 0xFE)
    __poke8(runtime + 56, 0x48)  // mov rdi, 0
    __poke8(runtime + 57, 0xC7)
    __poke8(runtime + 58, 0xC7)
    __poke8(runtime + 59, 0x00)
    __poke8(runtime + 60, 0x00)
    __poke8(runtime + 61, 0x00)
    __poke8(runtime + 62, 0x00)
    __poke8(runtime + 63, 0x48)  // mov rdx, 3
    __poke8(runtime + 64, 0xC7)
    __poke8(runtime + 65, 0xC2)
    __poke8(runtime + 66, 0x03)
    __poke8(runtime + 67, 0x00)
    __poke8(runtime + 68, 0x00)
    __poke8(runtime + 69, 0x00)
    __poke8(runtime + 70, 0x49)  // mov r10, 0x22
    __poke8(runtime + 71, 0xC7)
    __poke8(runtime + 72, 0xC2)
    __poke8(runtime + 73, 0x22)
    __poke8(runtime + 74, 0x00)
    __poke8(runtime + 75, 0x00)
    __poke8(runtime + 76, 0x00)
    __poke8(runtime + 77, 0x49)  // mov r8, -1
    __poke8(runtime + 78, 0xC7)
    __poke8(runtime + 79, 0xC0)
    __poke8(runtime + 80, 0xFF)
    __poke8(runtime + 81, 0xFF)
    __poke8(runtime + 82, 0xFF)
    __poke8(runtime + 83, 0xFF)
    __poke8(runtime + 84, 0x49)  // mov r9, 0
    __poke8(runtime + 85, 0xC7)
    __poke8(runtime + 86, 0xC1)
    __poke8(runtime + 87, 0x00)
    __poke8(runtime + 88, 0x00)
    __poke8(runtime + 89, 0x00)
    __poke8(runtime + 90, 0x00)
    __poke8(runtime + 91, 0x48)  // mov rax, 9
    __poke8(runtime + 92, 0xC7)
    __poke8(runtime + 93, 0xC0)
    __poke8(runtime + 94, 0x09)
    __poke8(runtime + 95, 0x00)
    __poke8(runtime + 96, 0x00)
    __poke8(runtime + 97, 0x00)
    __poke8(runtime + 98, 0x0F)  // syscall
    __poke8(runtime + 99, 0x05)
    __poke8(runtime + 100, 0xC3)  // ret

    let runtime_size = 101

    // Patch the call-to-main: find function named "main" by searching func_names
    let main_idx = 0
    let fnames = cg_func_names(cg)
    let search_i = 0
    while search_i < dyn_len(fnames) {
        if strcmp(dyn_get(fnames, search_i), "main") == 0 {
            main_idx = search_i
        }
        search_i = search_i + 1
    }
    let main_offset = dyn_get(cg_func_offsets(cg), main_idx)
    let rel = (runtime_size + main_offset) - (call_main_patch + 4)
    __poke32(runtime + call_main_patch, rel)

    // Full text segment = runtime + user code
    let full_text_len = runtime_size + text_len
    let full_text = alloc_bytes(full_text_len)
    memcpy(full_text, runtime, runtime_size)
    memcpy(full_text + runtime_size, text_data, text_len)

    // Compute vaddrs (needed for RIP-relative patches)
    let BASE = 0x40000000
    let PAGE = 0x1000
    let headers_end = 64 + 3 * 56
    let text_file_off = headers_end
    let text_vaddr = BASE + text_file_off

    // Patch call sites
    let patches = cg_call_patches(cg)
    let i = 0
    while i < dyn_len(patches) {
        let ppos = dyn_get(patches, i)
        let fidx = dyn_get(patches, i + 1)
        i = i + 2
        if fidx >= 100000 {
            // Runtime helper: encoding 100000 + rt_off
            let rt_off = fidx - 100000
            let target = rt_off
            let site = runtime_size + ppos
            let r = target - (site + 4)
            __poke8(full_text + site, r & 0xFF)
            __poke8(full_text + site + 1, (r >> 8) & 0xFF)
            __poke8(full_text + site + 2, (r >> 16) & 0xFF)
            __poke8(full_text + site + 3, (r >> 24) & 0xFF)
        } else {
            if fidx >= 0 {
                // Direct call to user function fidx
                let foff = dyn_get(cg_func_offsets(cg), fidx)
                let target = runtime_size + foff
                let site = runtime_size + ppos
                let r = target - (site + 4)
                __poke8(full_text + site, r & 0xFF)
                __poke8(full_text + site + 1, (r >> 8) & 0xFF)
                __poke8(full_text + site + 2, (r >> 16) & 0xFF)
                __poke8(full_text + site + 3, (r >> 24) & 0xFF)
            }
        }
    }

    // Build rodata (string literals)
    let strs = cg_strings(cg)
    let rodata_len = 0
    let j = 0
    while j < dyn_len(strs) {
        rodata_len = rodata_len + strlen(dyn_get(strs, j)) + 1
        j = j + 1
    }
    let rodata = alloc_bytes(rodata_len)
    let str_offsets = dyn_new()
    let off = 0
    j = 0
    while j < dyn_len(strs) {
        dyn_push(str_offsets, off)
        let s = dyn_get(strs, j)
        let slen = strlen(s)
        memcpy(rodata + off, s, slen)
        __poke8(rodata + off + slen, 0)
        off = off + slen + 1
        j = j + 1
    }

    // Patch string references
    let BASE = 0x40000000
    let PAGE = 0x1000
    let headers_end = 64 + 3 * 56
    let text_file_off = headers_end
    let text_vaddr = BASE + text_file_off
    let rodata_vaddr = align_up(text_vaddr + full_text_len, PAGE)
    let rodata_file_off = align_up(text_file_off + full_text_len, PAGE)
    let data_file_off = align_up(rodata_file_off + rodata_len, PAGE)
    let data_vaddr = align_up(rodata_vaddr + rodata_len, PAGE)

    // Patch string patches
    let spatches = cg_string_patches(cg)
    i = 0
    while i < dyn_len(spatches) {
        let ppos = dyn_get(spatches, i)
        let sidx = dyn_get(spatches, i + 1)
        i = i + 2
        let site_vaddr = text_vaddr + runtime_size + ppos
        let target_vaddr = rodata_vaddr + dyn_get(str_offsets, sidx)
        let r = target_vaddr - (site_vaddr + 4)
        let site = runtime_size + ppos
        __poke8(full_text + site, r & 0xFF)
        __poke8(full_text + site + 1, (r >> 8) & 0xFF)
        __poke8(full_text + site + 2, (r >> 16) & 0xFF)
        __poke8(full_text + site + 3, (r >> 24) & 0xFF)
    }

    // Patch function pointer patches (lea rax, [rip+disp])
    i = 0
    while i < dyn_len(patches) {
        let ppos = dyn_get(patches, i)
        let fidx = dyn_get(patches, i + 1)
        i = i + 2
        if fidx < 0 {
            let real_idx = 0 - 1 - fidx
            let target_vaddr = text_vaddr + runtime_size + dyn_get(cg_func_offsets(cg), real_idx)
            let site_vaddr = text_vaddr + runtime_size + ppos
            let r = target_vaddr - (site_vaddr + 4)
            let site = runtime_size + ppos
            __poke8(full_text + site, r & 0xFF)
            __poke8(full_text + site + 1, (r >> 8) & 0xFF)
            __poke8(full_text + site + 2, (r >> 16) & 0xFF)
            __poke8(full_text + site + 3, (r >> 24) & 0xFF)
        }
    }

    // Build ELF
    let entry = text_vaddr  // _start is at offset 0 in runtime
    let data_len = 0x1000
    let elf = buf_new()

    // ELF header (64 bytes)
    buf_push_byte(elf, 0x7F)
    buf_push_byte(elf, 69)  // 'E'
    buf_push_byte(elf, 76)  // 'L'
    buf_push_byte(elf, 70)  // 'F'
    buf_push_byte(elf, 2)   // ELFCLASS64
    buf_push_byte(elf, 1)   // ELFDATA2LSB
    buf_push_byte(elf, 1)   // EV_CURRENT
    buf_push_byte(elf, 0)   // SYSV
    let z = 0
    while z < 8 {
        buf_push_byte(elf, 0)
        z = z + 1
    }
    buf_push_u16(elf, 2)    // ET_EXEC
    buf_push_u16(elf, 0x3E) // EM_X86_64
    buf_push_u32(elf, 1)    // e_version
    buf_push_u64(elf, entry)
    buf_push_u64(elf, 64)   // e_phoff
    buf_push_u64(elf, 0)    // e_shoff
    buf_push_u32(elf, 0)    // e_flags
    buf_push_u16(elf, 64)   // e_ehsize
    buf_push_u16(elf, 56)   // e_phentsize
    buf_push_u16(elf, 3)    // e_phnum
    buf_push_u16(elf, 0)    // e_shentsize
    buf_push_u16(elf, 0)    // e_shnum
    buf_push_u16(elf, 0)    // e_shstrndx

    // Program headers (3 × 56 bytes)
    // .text: R+X
    buf_push_u32(elf, 1)    // p_type = LOAD
    buf_push_u32(elf, 5)    // p_flags = R+X
    buf_push_u64(elf, text_file_off)
    buf_push_u64(elf, text_vaddr)
    buf_push_u64(elf, text_vaddr)
    buf_push_u64(elf, full_text_len)
    buf_push_u64(elf, full_text_len)
    buf_push_u64(elf, PAGE)

    // .rodata: R
    buf_push_u32(elf, 1)
    buf_push_u32(elf, 4)    // R
    buf_push_u64(elf, rodata_file_off)
    buf_push_u64(elf, rodata_vaddr)
    buf_push_u64(elf, rodata_vaddr)
    buf_push_u64(elf, rodata_len)
    buf_push_u64(elf, rodata_len)
    buf_push_u64(elf, PAGE)

    // .data: R+W
    buf_push_u32(elf, 1)
    buf_push_u32(elf, 6)    // R+W
    buf_push_u64(elf, data_file_off)
    buf_push_u64(elf, data_vaddr)
    buf_push_u64(elf, data_vaddr)
    buf_push_u64(elf, data_len)
    buf_push_u64(elf, data_len)
    buf_push_u64(elf, PAGE)

    // Pad to text_file_off
    while buf_len(elf) < text_file_off {
        buf_push_byte(elf, 0)
    }
    // Append text
    buf_append(elf, full_text, full_text_len)
    // Pad to rodata_file_off
    while buf_len(elf) < rodata_file_off {
        buf_push_byte(elf, 0)
    }
    // Append rodata
    buf_append(elf, rodata, rodata_len)
    // Pad to data_file_off
    while buf_len(elf) < data_file_off {
        buf_push_byte(elf, 0)
    }
    // Append data (zeros)
    let dz = 0
    while dz < data_len {
        buf_push_byte(elf, 0)
        dz = dz + 1
    }

    // Write to file
    let total_len = buf_len(elf)
    let result = write_file(output_path, buf_data(elf), total_len)
    if result != 0 {
        die("emit: failed to write output file")
    }
    return total_len
}

// ===========================================================================
// MAIN
// ===========================================================================

fn main(argc, argv) {
    if argc < 2 {
        print_str("usage: qessac <input.qs> [output]\n")
        return 1
    }

    let input_path = __peek64(argv + 8)
    let output_path = "a.out"
    if argc >= 3 {
        output_path = __peek64(argv + 16)
    }

    print_str("Qessa v2.2\n")

    // Read input file
    let user_src = read_file(input_path)
    if user_src == 0 {
        print_str("error: cannot read ")
        print_str(input_path)
        print_str("\n")
        return 1
    }

    // Auto-import: try stdlib.qs from CWD, then from input file's directory
    let stdlib_src = read_file("stdlib.qs")
    if stdlib_src == 0 {
        // Try extracting directory from input_path
        let dlen = strlen(input_path)
        let slash_pos = 0 - 1
        let si = dlen - 1
        while si >= 0 {
            if __peek8(input_path + si) == 47 {
                slash_pos = si
                break
            }
            si = si - 1
        }
        if slash_pos >= 0 {
            let sp = alloc_bytes(slash_pos + 12)
            memcpy(sp, input_path, slash_pos + 1)
            memcpy(sp + slash_pos + 1, "stdlib.qs", 10)
            stdlib_src = read_file(sp)
        }
    }
    let src = 0
    if stdlib_src != 0 {
        // Prepend stdlib to user source
        let slen = strlen(stdlib_src)
        let ulen = strlen(user_src)
        src = alloc_bytes(slen + ulen + 1)
        memcpy(src, stdlib_src, slen)
        memcpy(src + slen, user_src, ulen)
        __poke8(src + slen + ulen, 0)
    } else {
        // No stdlib found, use user source as-is
        src = user_src
    }

    print_str("read ")
    print_str(input_path)
    print_str(" (")
    print_int(strlen(src))
    print_str(" bytes)\n")

    // Lex
    let tokens = lex_all(src)
    print_str("lexed: ")
    print_int(dyn_len(tokens))
    print_str(" tokens\n")

    // Parse
    let parser = parser_new(tokens)
    let prog = parse_program(parser)
    let funcs = ast_c1(prog)
    print_str("parsed: ")
    print_int(dyn_len(funcs))
    print_str(" functions\n")

    // Codegen
    let cg = cg_new()
    cg_compile_program(cg, prog)
    print_str("generated: ")
    print_int(buf_len(cg_text(cg)))
    print_str(" bytes\n")

    // Emit
    let elf_size = emit_elf(cg, output_path)
    syscall_write(1, "done\n", 5)
    return 0
}
