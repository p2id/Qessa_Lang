// cat.qs — File concatenation utility written in Qessa
fn main(argc, argv) {
    if argc < 2 {
        print_str("usage: cat <file>\n")
        return 1
    }
    let path = __peek64(argv + 8)
    let content = read_file(path)
    if content == 0 {
        print_str("cat: cannot read file\n")
        return 1
    }
    print_str(content)
    return 0
}
