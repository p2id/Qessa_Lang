// enums.qs — test enums and match
enum Color {
    Red
    Green
    Blue
}

enum Shape {
    Circle
    Square
    Triangle
}

fn color_name(c) {
    if c == Color::Red {
        syscall_write(1, "Red\n", 4)
    } else {
        if c == Color::Green {
            syscall_write(1, "Green\n", 6)
        } else {
            if c == Color::Blue {
                syscall_write(1, "Blue\n", 5)
            } else {
                syscall_write(1, "Unknown\n", 8)
            }
        }
    }
    return 0
}

fn main() {
    syscall_write(1, "start\n", 6)
    let r = Color::Red
    let g = Color::Green
    let b = Color::Blue
    color_name(r)
    color_name(g)
    color_name(b)

    // Test match
    let s = Shape::Circle
    match s {
        Shape::Circle => {
            syscall_write(1, "circle\n", 7)
        }
        Shape::Square => {
            syscall_write(1, "square\n", 7)
        }
        Shape::Triangle => {
            syscall_write(1, "triangle\n", 9)
        }
    }
    return 0
}
