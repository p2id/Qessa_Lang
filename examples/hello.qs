// hello.qs — first Qessa program
// Prints "Hello, World!\n" via direct Linux syscall.

fn main() {
    syscall_write(1, "Hello, World!\n", 14)
    return 0
}
