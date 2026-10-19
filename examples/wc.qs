// wc.qs — Word/line/char counter written in Qessa
fn main(argc, argv) {
    if argc < 2 {
        print_str("usage: wc <file>\n")
        return 1
    }
    let path = __peek64(argv + 8)
    let content = read_file(path)
    if content == 0 {
        print_str("wc: cannot read file\n")
        return 1
    }
    
    let chars = 0
    let lines = 0
    let words = 0
    let in_word = 0
    let p = content
    let c = __peek8(p)
    
    while c != 0 {
        chars = chars + 1
        if c == 10 {
            lines = lines + 1
            in_word = 0
        } else {
            if c == 32 || c == 9 {
                in_word = 0
            } else {
                if in_word == 0 {
                    words = words + 1
                    in_word = 1
                }
            }
        }
        p = p + 1
        c = __peek8(p)
    }
    
    print_str("lines: ")
    print_int(lines)
    print_str("\nwords: ")
    print_int(words)
    print_str("\nchars: ")
    print_int(chars)
    print_str("\n")
    return 0
}
