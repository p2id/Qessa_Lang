// modules.qs — test modules and cross-module calls
mod math {
    fn add(a, b) {
        return a + b
    }
    fn mul(a, b) {
        return a * b
    }
    fn factorial(n) {
        if n <= 1 {
            return 1
        }
        return n * factorial(n - 1)
    }
}

mod io {
    fn print_ok() {
        syscall_write(1, "OK\n", 3)
    }
    fn print_fail() {
        syscall_write(1, "FAIL\n", 5)
    }
}

fn main() {
    syscall_write(1, "start\n", 6)

    // math::add(3, 4) = 7
    let s = math::add(3, 4)
    if s == 7 {
        io::print_ok()
    } else {
        io::print_fail()
    }

    // math::mul(6, 7) = 42
    let p = math::mul(6, 7)
    if p == 42 {
        io::print_ok()
    } else {
        io::print_fail()
    }

    // math::factorial(5) = 120
    let f = math::factorial(5)
    if f == 120 {
        io::print_ok()
    } else {
        io::print_fail()
    }

    return 0
}
