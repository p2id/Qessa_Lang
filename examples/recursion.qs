// recursion.qs — test recursion
fn fib(n) {
    if n < 2 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    let r = fib(10)
    // r should be 55. We'll print it as a literal string for now.
    if r == 55 {
        syscall_write(1, "fib(10)=55 OK\n", 13)
    } else {
        syscall_write(1, "fib(10) WRONG\n", 14)
    }
    return 0
}
