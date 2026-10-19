fn factorial(n) {
    if n <= 1 {
        return 1
    }
    return n * factorial(n - 1)
}

fn main() {
    let f5 = factorial(5)
    if f5 == 120 {
        syscall_write(1, "f5=120 OK\n", 10)
    } else {
        syscall_write(1, "FAIL\n", 5)
    }
    return 0
}
