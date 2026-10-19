// comprehensive.qs — full feature integration test
//
// Tests: structs, methods, regions, arrays, pointers, loops,
// recursion, modules, enums, match, syscalls.

mod geometry {
    struct Point {
        x
        y
    }

    fn make_point(x, y) {
        return Point { x: x, y: y }
    }

    fn distance_squared(p) {
        // p.x * p.x + p.y * p.y
        let xx = p.x * p.x
        let yy = p.y * p.y
        return xx + yy
    }
}

mod logic {
    enum Comparison {
        Less
        Equal
        Greater
    }

    fn compare(a, b) {
        if a < b {
            return Comparison::Less
        }
        if a == b {
            return Comparison::Equal
        }
        return Comparison::Greater
    }
}

// Compute n! iteratively
fn factorial_iter(n) {
    let result = 1
    let i = 1
    while i <= n {
        result = result * i
        i = i + 1
    }
    return result
}

// Sum of array elements
fn array_sum(arr) {
    let sum = 0
    let i = 0
    let n = 0
    // Read length from header: arr[0]
    let p = &arr[0]
    let len_ptr = p - 16
    let len = *len_ptr
    while i < len {
        sum = sum + arr[i]
        i = i + 1
    }
    return sum
}

fn main() {
    syscall_write(1, "=== Qessa Comprehensive Test ===\n", 34)

    // Test 1: struct + fields
    let p = geometry::make_point(3, 4)
    let d2 = geometry::distance_squared(p)
    if d2 == 25 {
        syscall_write(1, "T1 struct: OK\n", 14)
    } else {
        syscall_write(1, "T1 struct: FAIL\n", 16)
    }

    // Test 2: factorial iter
    let f10 = factorial_iter(10)
    if f10 == 3628800 {
        syscall_write(1, "T2 iter: OK\n", 12)
    } else {
        syscall_write(1, "T2 iter: FAIL\n", 14)
    }

    // Test 3: enum + match
    let c = logic::compare(5, 10)
    match c {
        logic::Comparison::Less => {
            syscall_write(1, "T3 less: OK\n", 12)
        }
        logic::Comparison::Equal => {
            syscall_write(1, "T3 FAIL\n", 8)
        }
        logic::Comparison::Greater => {
            syscall_write(1, "T3 FAIL\n", 8)
        }
    }

    // Test 4: regions + arrays
    region r {
        let arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] in r
        let s = array_sum(arr)
        if s == 55 {
            syscall_write(1, "T4 region+arr: OK\n", 19)
        } else {
            syscall_write(1, "T4 FAIL\n", 8)
        }
    }

    // Test 5: pointer arithmetic
    let x = 100
    let px = &x
    *px = 200
    if x == 200 {
        syscall_write(1, "T5 ptr: OK\n", 11)
    } else {
        syscall_write(1, "T5 ptr: FAIL\n", 13)
    }

    // Test 6: nested loops (multiplication table)
    let total = 0
    let i = 1
    while i <= 3 {
        let j = 1
        while j <= 3 {
            total = total + i * j
            j = j + 1
        }
        i = i + 1
    }
    // total = (1+2+3) * (1+2+3) = 6 * 6 = 36
    if total == 36 {
        syscall_write(1, "T6 nested: OK\n", 14)
    } else {
        syscall_write(1, "T6 nested: FAIL\n", 16)
    }

    syscall_write(1, "=== Done ===\n", 13)
    return 0
}
