fn main(argc, argv) {
    let a = 10
    let b = 20
    let c = a + b
    if c == 30 {
        syscall_write(1, "add OK\n", 7)
    }
    let d = 6 * 7
    if d == 42 {
        syscall_write(1, "mul OK\n", 7)
    }
    let e = 100 / 7
    if e == 14 {
        syscall_write(1, "div OK\n", 7)
    }
    let i = 0
    let sum = 0
    while i < 10 {
        sum = sum + i
        i = i + 1
    }
    if sum == 45 {
        syscall_write(1, "loop OK\n", 7)
    }
    let result = triple(5)
    if result == 15 {
        syscall_write(1, "call OK\n", 7)
    }
    syscall_write(1, "done\n", 5)
    return 0
}

fn triple(n) {
    return n * 3
}
