// pointers.qs — test pointers and address-of
fn main() {
    syscall_write(1, "start\n", 6)

    let x = 42
    let p = &x       // p is a pointer to x
    let v = *p       // dereference p, should be 42

    if v == 42 {
        syscall_write(1, "deref=42 OK\n", 12)
    } else {
        syscall_write(1, "FAIL\n", 5)
    }

    // Modify through pointer
    *p = 99
    if x == 99 {
        syscall_write(1, "mut=99 OK\n", 11)
    } else {
        syscall_write(1, "FAIL\n", 5)
    }

    // Test for-loop over array
    let arr = [1, 2, 3, 4, 5]
    let sum = 0
    for v in arr {
        sum = sum + v
    }
    if sum == 15 {
        syscall_write(1, "for_sum=15 OK\n", 14)
    } else {
        syscall_write(1, "FAIL\n", 5)
    }

    return 0
}
