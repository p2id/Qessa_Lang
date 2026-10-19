// regions.qs — test region-based memory allocation
fn main() {
    syscall_write(1, "start\n", 6)

    // Allocate a region and put arrays inside it
    region r {
        let a = [1, 2, 3, 4, 5] in r
        let b = [10, 20, 30] in r

        let sum_a = 0
        let i = 0
        while i < 5 {
            sum_a = sum_a + a[i]
            i = i + 1
        }

        let sum_b = 0
        let j = 0
        while j < 3 {
            sum_b = sum_b + b[j]
            j = j + 1
        }

        // sum_a = 15, sum_b = 60, total = 75
        let total = sum_a + sum_b
        if total == 75 {
            syscall_write(1, "total=75 OK\n", 12)
        } else {
            syscall_write(1, "FAIL\n", 5)
        }
    }
    // Region freed here automatically

    syscall_write(1, "after\n", 6)
    return 0
}
