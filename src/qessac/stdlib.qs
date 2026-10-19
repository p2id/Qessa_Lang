// stdlib.qs — Qessa standard library
// Helper functions for string manipulation, I/O, hashing, and number formatting.
// Used by the self-hosted compiler (qessac.qs).

// ---------------------------------------------------------------------------
// String length: strlen(s) → length in bytes (excluding null terminator)
// ---------------------------------------------------------------------------
fn strlen(s) {
    let n = 0
    let p = s
    while __peek8(p) != 0 {
        p = p + 1
        n = n + 1
    }
    return n
}

// ---------------------------------------------------------------------------
// String compare: strcmp(a, b) → 0 if equal, negative if a<b, positive if a>b
// ---------------------------------------------------------------------------
fn strcmp(a, b) {
    let pa = a
    let pb = b
    while __peek8(pa) != 0 {
        if __peek8(pb) == 0 {
            return 1
        }
        if __peek8(pa) < __peek8(pb) {
            return -1
        }
        if __peek8(pa) > __peek8(pb) {
            return 1
        }
        pa = pa + 1
        pb = pb + 1
    }
    if __peek8(pb) == 0 {
        return 0
    }
    return -1
}

// ---------------------------------------------------------------------------
// Copy n bytes: memcpy(dst, src, n)
// ---------------------------------------------------------------------------
fn memcpy(dst, src, n) {
    let i = 0
    while i < n {
        __poke8(dst + i, __peek8(src + i))
        i = i + 1
    }
    return dst
}

// ---------------------------------------------------------------------------
// FNV-1a hash (32-bit folded to avoid i64 overflow issues): hash_str(s) → i64
// ---------------------------------------------------------------------------
fn hash_str(s) {
    // Use a 32-bit FNV-1a hash, sign-extended to 64 bits.
    // 32-bit offset basis: 0x811C9DC5 = 2166136261 (fits in u32, but we treat as i64)
    // To avoid overflow, we use 2166136261 as the starting value and mask to 32 bits.
    let h = 2166136261
    let p = s
    let c = __peek8(p)
    while c != 0 {
        h = h ^ c
        h = h * 16777619
        // Mask to 32 bits to prevent overflow (keep only low 32 bits)
        h = h & 0xFFFFFFFF
        p = p + 1
        c = __peek8(p)
    }
    return h
}

// ---------------------------------------------------------------------------
// Parse decimal integer: atou(s) → i64
// ---------------------------------------------------------------------------
fn atou(s) {
    let result = 0
    let p = s
    let c = __peek8(p)
    while c >= 48 && c <= 57 {
        result = result * 10 + (c - 48)
        p = p + 1
        c = __peek8(p)
    }
    return result
}

// ---------------------------------------------------------------------------
// Format unsigned integer as decimal: uitoa(n, buf) → length
// ---------------------------------------------------------------------------
fn uitoa(n, buf) {
    if n == 0 {
        __poke8(buf, 48)
        __poke8(buf + 1, 0)
        return 1
    }
    let tmp = alloc_bytes(32)
    let len = 0
    let x = n
    while x > 0 {
        let d = x % 10
        __poke8(tmp + len, d + 48)
        len = len + 1
        x = x / 10
    }
    let i = 0
    while i < len {
        __poke8(buf + i, __peek8(tmp + len - 1 - i))
        i = i + 1
    }
    __poke8(buf + len, 0)
    return len
}

// ---------------------------------------------------------------------------
// Format signed integer as decimal: itoa(n, buf) → length
// ---------------------------------------------------------------------------
fn itoa(n, buf) {
    if n < 0 {
        __poke8(buf, 45)
        let len = uitoa(0 - n, buf + 1)
        return len + 1
    }
    return uitoa(n, buf)
}

// ---------------------------------------------------------------------------
// Print integer to stdout: print_int(n)
// ---------------------------------------------------------------------------
fn print_int(n) {
    let buf = alloc_bytes(32)
    let len = itoa(n, buf)
    syscall_write(1, buf, len)
    return 0
}

// ---------------------------------------------------------------------------
// Print string to stdout: print_str(s)
// ---------------------------------------------------------------------------
fn print_str(s) {
    let len = strlen(s)
    syscall_write(1, s, len)
    return 0
}

// ---------------------------------------------------------------------------
// Allocate a new string of length n (zero-filled): str_new(n) → ptr
// ---------------------------------------------------------------------------
fn str_new(n) {
    let p = alloc_bytes(n + 1)
    let i = 0
    while i <= n {
        __poke8(p + i, 0)
        i = i + 1
    }
    return p
}

// ---------------------------------------------------------------------------
// Concatenate two strings: str_concat(a, b) → new string
// ---------------------------------------------------------------------------
fn str_concat(a, b) {
    let la = strlen(a)
    let lb = strlen(b)
    let result = str_new(la + lb)
    memcpy(result, a, la)
    memcpy(result + la, b, lb)
    return result
}

// ---------------------------------------------------------------------------
// Allocate n bytes of zeroed memory: alloc_bytes(n) → ptr
// Uses mmap directly (avoids alloc_bytes which had alignment issues).
// ---------------------------------------------------------------------------
fn alloc_bytes(n) {
    // Round n up to page size (4096)
    let pages = (n + 4095) / 4096
    let size = pages * 4096
    let p = syscall_mmap(0, size, 3, 34, -1, 0)
    if p < 0 {
        return 0
    }
    return p
}

// ---------------------------------------------------------------------------
// Read entire file: read_file(path) → string pointer (or 0 on error)
// ---------------------------------------------------------------------------
fn read_file(path) {
    let fd = syscall_open(path, 0, 0)
    if fd < 0 {
        return 0
    }
    let capacity = 4096
    let buf = alloc_bytes(capacity)
    if buf == 0 {
        syscall_close(fd)
        return 0
    }
    let total = 0
    while 1 {
        let available = capacity - total - 1
        if available <= 0 {
            let new_capacity = capacity * 2
            let new_buf = alloc_bytes(new_capacity)
            if new_buf == 0 {
                syscall_close(fd)
                return 0
            }
            memcpy(new_buf, buf, total)
            buf = new_buf
            capacity = new_capacity
            available = capacity - total - 1
        }
        let chunk = syscall_read(fd, buf + total, available)
        if chunk < 0 {
            syscall_close(fd)
            return 0
        }
        if chunk == 0 {
            break
        }
        total = total + chunk
    }
    syscall_close(fd)
    __poke8(buf + total, 0)
    return buf
}

// ---------------------------------------------------------------------------
// Write buffer to file: write_file(path, data, len) → 0 on success, -1 on error
// ---------------------------------------------------------------------------
fn write_file(path, data, len) {
    let fd = syscall_open(path, 0x241, 493)
    if fd < 0 {
        return -1
    }
    let total = 0
    while total < len {
        let written = syscall_write(fd, data + total, len - total)
        if written <= 0 {
            syscall_close(fd)
            return -1
        }
        total = total + written
    }
    syscall_close(fd)
    return 0
}

// ---------------------------------------------------------------------------
// Dynamic byte buffer (for building machine code / ELF)
// Layout: [ptr, len, cap]  (3 i64s = 24 bytes)
// ---------------------------------------------------------------------------

fn buf_new() {
    let b = alloc_bytes(24)
    let data = alloc_bytes(1048576)
    __poke64(b, data)
    __poke64(b + 8, 0)
    __poke64(b + 16, 1048576)
    return b
}

fn buf_data(b) {
    return __peek64(b)
}

fn buf_len(b) {
    return __peek64(b + 8)
}

fn buf_cap(b) {
    return __peek64(b + 16)
}

fn buf_push_byte(b, byte) {
    let len = buf_len(b)
    let cap = buf_cap(b)
    if len >= cap {
        let new_cap = cap * 2
        let new_data = alloc_bytes(new_cap)
        let old_data = buf_data(b)
        memcpy(new_data, old_data, len)
        __poke64(b, new_data)
        __poke64(b + 16, new_cap)
    }
    let data = buf_data(b)
    __poke8(data + len, byte)
    __poke64(b + 8, len + 1)
    return b
}

fn buf_push_u16(b, v) {
    buf_push_byte(b, v & 0xFF)
    buf_push_byte(b, (v >> 8) & 0xFF)
    return b
}

fn buf_push_u32(b, v) {
    buf_push_byte(b, v & 0xFF)
    buf_push_byte(b, (v >> 8) & 0xFF)
    buf_push_byte(b, (v >> 16) & 0xFF)
    buf_push_byte(b, (v >> 24) & 0xFF)
    return b
}

fn buf_push_u64(b, v) {
    buf_push_byte(b, v & 0xFF)
    buf_push_byte(b, (v >> 8) & 0xFF)
    buf_push_byte(b, (v >> 16) & 0xFF)
    buf_push_byte(b, (v >> 24) & 0xFF)
    buf_push_byte(b, (v >> 32) & 0xFF)
    buf_push_byte(b, (v >> 40) & 0xFF)
    buf_push_byte(b, (v >> 48) & 0xFF)
    buf_push_byte(b, (v >> 56) & 0xFF)
    return b
}

fn buf_push_i32(b, v) {
    return buf_push_u32(b, v)
}

fn buf_push_i64(b, v) {
    return buf_push_u64(b, v)
}

fn buf_get_byte(b, i) {
    let data = buf_data(b)
    return __peek8(data + i)
}

fn buf_set_byte(b, i, v) {
    let data = buf_data(b)
    __poke8(data + i, v)
    return b
}

fn buf_get_u32(b, i) {
    let data = buf_data(b)
    let a = __peek8(data + i)
    let bb = __peek8(data + i + 1)
    let c = __peek8(data + i + 2)
    let d = __peek8(data + i + 3)
    return a | (bb << 8) | (c << 16) | (d << 24)
}

fn buf_set_u32(b, i, v) {
    let data = buf_data(b)
    __poke8(data + i, v & 0xFF)
    __poke8(data + i + 1, (v >> 8) & 0xFF)
    __poke8(data + i + 2, (v >> 16) & 0xFF)
    __poke8(data + i + 3, (v >> 24) & 0xFF)
    return b
}

fn buf_get_u64(b, i) {
    let data = buf_data(b)
    let r = 0
    let k = 0
    while k < 8 {
        r = r | (__peek8(data + i + k) << (k * 8))
        k = k + 1
    }
    return r
}

fn buf_set_u64(b, i, v) {
    let data = buf_data(b)
    let k = 0
    while k < 8 {
        __poke8(data + i + k, (v >> (k * 8)) & 0xFF)
        k = k + 1
    }
    return b
}

fn buf_append(b, src, n) {
    let i = 0
    while i < n {
        buf_push_byte(b, __peek8(src + i))
        i = i + 1
    }
    return b
}

// ---------------------------------------------------------------------------
// Dynamic array (i64 elements)
// Layout: [ptr, len, cap]
// ---------------------------------------------------------------------------

fn dyn_new() {
    let d = alloc_bytes(24)
    let data = alloc_bytes(524288)
    __poke64(d, data)
    __poke64(d + 8, 0)
    __poke64(d + 16, 65536)
    return d
}

fn dyn_ptr(d) {
    return __peek64(d)
}

fn dyn_len(d) {
    return __peek64(d + 8)
}

fn dyn_cap(d) {
    return __peek64(d + 16)
}

fn dyn_push(d, v) {
    let len = dyn_len(d)
    let cap = dyn_cap(d)
    if len >= cap {
        let new_cap = cap * 2
        let new_data = alloc_bytes(new_cap * 8)
        let old_data = dyn_ptr(d)
        let i = 0
        while i < len {
            __poke64(new_data + i * 8, __peek64(old_data + i * 8))
            i = i + 1
        }
        __poke64(d, new_data)
        __poke64(d + 16, new_cap)
    }
    let data = dyn_ptr(d)
    __poke64(data + len * 8, v)
    __poke64(d + 8, len + 1)
    return d
}

fn dyn_get(d, i) {
    let data = dyn_ptr(d)
    return __peek64(data + i * 8)
}

fn dyn_set(d, i, v) {
    let data = dyn_ptr(d)
    __poke64(data + i * 8, v)
    return d
}

// ---------------------------------------------------------------------------
// String table: hash map for interned strings (linear probing)
// Layout: [entries_ptr, count, capacity]
// Each entry: [key_ptr, value]  (16 bytes)
// ---------------------------------------------------------------------------

fn strtab_new() {
    let t = alloc_bytes(24)
    let cap = 256
    let entries = alloc_bytes(cap * 16)
    let i = 0
    while i < cap * 16 {
        __poke8(entries + i, 0)
        i = i + 1
    }
    __poke64(t, entries)
    __poke64(t + 8, 0)
    __poke64(t + 16, cap)
    return t
}

fn strtab_entries(t) {
    return __peek64(t)
}

fn strtab_count(t) {
    return __peek64(t + 8)
}

fn strtab_cap(t) {
    return __peek64(t + 16)
}

fn strtab_find_slot(t, key) {
    let cap = strtab_cap(t)
    let entries = strtab_entries(t)
    let h = hash_str(key)
    let idx = h & (cap - 1)
    let i = 0
    while i < cap {
        let slot = entries + idx * 16
        let existing = __peek64(slot)
        if existing == 0 {
            return slot
        }
        if strcmp(existing, key) == 0 {
            return slot
        }
        idx = (idx + 1) & (cap - 1)
        i = i + 1
    }
    return 0
}

fn strtab_intern(t, key) {
    let slot = strtab_find_slot(t, key)
    if slot == 0 {
        return 0
    }
    let existing = __peek64(slot)
    if existing == 0 {
        let len = strlen(key)
        let copy = str_new(len)
        memcpy(copy, key, len)
        __poke64(slot, copy)
        let count = strtab_count(t)
        __poke64(t + 8, count + 1)
        return copy
    }
    return existing
}

fn strtab_get(t, key) {
    let slot = strtab_find_slot(t, key)
    if slot == 0 {
        return 0
    }
    return __peek64(slot + 8)
}

fn strtab_set(t, key, value) {
    let slot = strtab_find_slot(t, key)
    if slot == 0 {
        return 0
    }
    let existing = __peek64(slot)
    if existing == 0 {
        let len = strlen(key)
        let copy = str_new(len)
        memcpy(copy, key, len)
        __poke64(slot, copy)
        let count = strtab_count(t)
        __poke64(t + 8, count + 1)
    }
    __poke64(slot + 8, value)
    return value
}

// ---------------------------------------------------------------------------
// Error reporting
// ---------------------------------------------------------------------------

fn die(msg) {
    print_str(msg)
    syscall_write(1, "\n", 1)
    syscall_exit(1)
    return 0
}

// ---------------------------------------------------------------------------
// String equality: streq(a, b) → 1 if equal, 0 if not
// ---------------------------------------------------------------------------
fn streq(a, b) {
    let pa = a
    let pb = b
    while __peek8(pa) != 0 {
        if __peek8(pb) == 0 { return 0 }
        if __peek8(pa) != __peek8(pb) { return 0 }
        pa = pa + 1
        pb = pb + 1
    }
    if __peek8(pb) == 0 { return 1 }
    return 0
}

// ---------------------------------------------------------------------------
// Array helpers
// ---------------------------------------------------------------------------
fn array_len(arr) {
    return __peek64(arr)
}

fn array_get(arr, i) {
    return __peek64(arr + 16 + i * 8)
}

fn array_set(arr, i, v) {
    __poke64(arr + 16 + i * 8, v)
    return arr
}

fn array_new(cap) {
    let arr = alloc_bytes(16 + cap * 8)
    __poke64(arr, 0)
    __poke64(arr + 8, cap)
    return arr
}

fn array_push(arr, v) {
    let len = __peek64(arr)
    let cap = __peek64(arr + 8)
    if len < cap {
        __poke64(arr + 16 + len * 8, v)
        __poke64(arr, len + 1)
    }
    return arr
}
