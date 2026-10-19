#!/usr/bin/env python3
"""QES-SEC-001 consistency check (no Rust required).

Verifies that:
  1. every syscall the self-hosted compiler knows exists in the Rust table with the same number,
  2. the self-hosted compiler has no default/fallback number for unknown names,
  3. the Rust table matches the Linux x86-64 numbers listed in LINUX_X86_64 below
     (transcribed from the kernel's syscall_64.tbl by the author; re-verify against the
     kernel source before release),
  4. selfhost's two internal tables (syscall_num and the inline chain) agree.
Exit code 0 = all checks pass, 1 = failure.
"""
import re, sys, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
rust = (ROOT / "src/codegen/mod.rs").read_text(encoding="utf-8")
qs = (ROOT / "src/qessac/qessac.qs").read_text(encoding="utf-8")

LINUX_X86_64 = {
    "read": 0, "write": 1, "open": 2, "close": 3, "stat": 4, "fstat": 5, "lstat": 6,
    "poll": 7, "lseek": 8, "mmap": 9, "mprotect": 10, "munmap": 11, "brk": 12,
    "ioctl": 16, "access": 21, "pipe": 22, "select": 23, "dup": 32, "dup2": 33,
    "nanosleep": 35, "getpid": 39, "fork": 57, "execve": 59, "exit": 60, "wait4": 61,
    "kill": 62, "uname": 63, "getcwd": 79, "chdir": 80, "fchdir": 81, "mkdir": 83,
    "rmdir": 84, "creat": 85, "link": 86, "unlink": 87, "symlink": 88, "readlink": 89,
    "chmod": 90, "fchmod": 91, "chown": 92, "fchown": 93, "umask": 95,
    "gettimeofday": 96, "getuid": 102, "getgid": 104,
}

errors = []

m = re.search(r"fn syscall_number\(.*?\n\}\n", rust, re.S)
if not m:
    sys.exit("cannot locate syscall_number in src/codegen/mod.rs")
rust_tbl = {n: int(v) for n, v in re.findall(r'"syscall_(\w+)"\s*=>\s*(\d+)', m.group(0))}

fn = re.search(r"fn syscall_num\(name\) \{.*?\n\}\n", qs, re.S)
inline = re.search(r"let num = (-?\d+).*?// syscall_: emit direct", qs, re.S)
if not fn or not inline:
    sys.exit("cannot locate selfhost syscall tables in src/qessac/qessac.qs")
fn_tbl = {n: int(v) for n, v in re.findall(r'"syscall_(\w+)"\) == 0 \{ return (\d+) \}', fn.group(0))}
fn_tbl_first = re.search(r'let r = strcmp\(name, "syscall_(\w+)"\)\s*\n\s*if r == 0 \{ return (\d+) \}', fn.group(0))
if fn_tbl_first:
    fn_tbl[fn_tbl_first.group(1)] = int(fn_tbl_first.group(2))
inl_tbl = {n: int(v) for n, v in re.findall(r'"syscall_(\w+)"\) == 0 \{ num = (\d+) \}', inline.group(0))}

# 1. selfhost subset of Rust with identical numbers
for name, num in sorted(inl_tbl.items()):
    if name not in rust_tbl:
        errors.append(f"selfhost knows syscall_{name} but Rust does not")
    elif rust_tbl[name] != num:
        errors.append(f"syscall_{name}: selfhost={num} rust={rust_tbl[name]}")
# 2. no fallback to a valid syscall
if int(inline.group(1)) >= 0:
    errors.append(f"selfhost inline table defaults to {inline.group(1)} (must be negative = reject)")
if not re.search(r"return -1\s*// unknown", fn.group(0)):
    errors.append("selfhost syscall_num() does not return -1 for unknown names")
if "error: unknown syscall" not in qs:
    errors.append("selfhost has no 'unknown syscall' diagnostic")
# 3. Rust table vs kernel numbers
for name, num in sorted(rust_tbl.items()):
    if name not in LINUX_X86_64:
        errors.append(f"syscall_{name} missing from reference list")
    elif LINUX_X86_64[name] != num:
        errors.append(f"syscall_{name}: rust={num} linux_x86_64={LINUX_X86_64[name]}")
# 4. internal selfhost consistency
if fn_tbl != inl_tbl:
    errors.append(f"selfhost internal tables differ: {sorted(set(fn_tbl.items()) ^ set(inl_tbl.items()))}")

print(f"rust: {len(rust_tbl)} syscalls; selfhost: {len(inl_tbl)} syscalls "
      f"(missing in selfhost: {len(set(rust_tbl) - set(inl_tbl))})")
if errors:
    print("FAIL"); [print(" -", e) for e in errors]; sys.exit(1)
print("OK")
