// simple_loop.qs — test basic loop and print
fn main() {
    let i = 0
    while i < 5 {
        syscall_write(1, "A", 1)
        i = i + 1
    }
    syscall_write(1, "\n", 1)
    return 0
}
