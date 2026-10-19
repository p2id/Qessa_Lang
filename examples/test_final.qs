// test_final.qs — Qessa v3.0 final comprehensive test
fn main() {
    syscall_write(1, "=== Qessa v3.0 Final Test ===\n", 30)

    // Arithmetic
    if 10 + 20 == 30 { syscall_write(1, "arith: OK\n", 10) }
    
    // Bitwise + hex
    if 0xFF & 0x0F == 0x0F { syscall_write(1, "bit: OK\n", 8) }
    if 1 << 4 == 16 { syscall_write(1, "shift: OK\n", 10) }
    
    // While loop
    let i = 0
    let sum = 0
    while i < 10 { sum = sum + i; i = i + 1 }
    if sum == 45 { syscall_write(1, "while: OK\n", 10) }
    
    // For loop + array
    let arr = [10, 20, 30, 40, 50]
    let asum = 0
    for x in arr { asum = asum + x }
    if asum == 150 { syscall_write(1, "for+arr: OK\n", 12) }
    
    // Array indexing + assignment
    arr[0] = 99
    if arr[0] == 99 { syscall_write(1, "arridx: OK\n", 11) }
    
    // Break in for
    arr[0] = 10
    let bsum = 0
    for x in arr { if x > 30 { break }; bsum = bsum + x }
    if bsum == 60 { syscall_write(1, "break: OK\n", 10) }
    
    // Compound assignment
    let ca = 10
    ca += 5
    ca *= 2
    if ca == 30 { syscall_write(1, "compound: OK\n", 13) }
    
    // Continue
    let csum = 0
    let ci = 0
    while ci < 10 { ci += 1; if ci == 5 { continue }; csum += ci }
    if csum == 50 { syscall_write(1, "continue: OK\n", 14) }
    
    // Recursion
    if fib(10) == 55 { syscall_write(1, "fib: OK\n", 8) }
    
    // Pointers
    let pv = 42
    let pp = &pv
    let ptr_before = *pp == 42
    if *pp == 42 { syscall_write(1, "ptr: OK\n", 8) }
    *pp = 99
    if pv == 99 { syscall_write(1, "ptrwr: OK\n", 10) }
    
    // String equality
    if streq("hello", "hello") { syscall_write(1, "streq: OK\n", 11) }
    
    // Logical operators
    if 1 && 1 { if 0 || 1 { if !0 { syscall_write(1, "logic: OK\n", 10) } } }
    
    // Negative numbers
    let neg = -7
    if neg < 0 { syscall_write(1, "neg: OK\n", 8) }
    
    if 10 + 20 == 30 && 0xFF & 0x0F == 0x0F && 1 << 4 == 16 && sum == 45 && asum == 150 && arr[0] == 10 && bsum == 60 && ca == 30 && csum == 50 && fib(10) == 55 && ptr_before && pv == 99 && streq("hello", "hello") && (1 && 1) && (0 || 1) && !0 && neg < 0 {
        syscall_write(1, "=== All Passed ===\n", 19)
    } else {
        syscall_write(1, "=== TEST FAILURE ===\n", 22)
        return 1
    }
    return 0
}

fn fib(n) {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}
