// arrays.qs — test array creation, indexing, iteration
fn main() {
    syscall_write(1, "start\n", 6)
    let arr = [10, 20, 30, 40, 50]
    let sum = 0
    let i = 0
    while i < 5 {
        sum = sum + arr[i]
        i = i + 1
    }
    // sum should be 150
    if sum == 150 {
        syscall_write(1, "sum=150 OK\n", 11)
    } else {
        syscall_write(1, "FAIL\n", 5)
    }
    return 0
}
