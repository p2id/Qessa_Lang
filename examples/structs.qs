// structs.qs — test struct allocation and field access
struct Point {
    x
    y
}

fn make_point(x, y) {
    let p = Point { x: x, y: y }
    return p
}

fn main() {
    syscall_write(1, "start\n", 6)
    let p = make_point(3, 4)
    let sum = p.x + p.y
    // sum should be 7
    if sum == 7 {
        syscall_write(1, "sum=7 OK\n", 9)
    } else {
        syscall_write(1, "FAIL\n", 5)
    }
    return 0
}
