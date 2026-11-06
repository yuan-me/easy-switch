"""Run static Linux test executables in an offline, RAM-only QEMU guest.

QEMU and an Alpine virt kernel/initramfs are supplied by the caller. Nothing is
installed into the host, and no host directory, disk or network is shared.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import unittest


def safe_name(name):
    if name.startswith("./"):
        name = name[2:]
    if name in ("", "."):
        return "."
    if name.startswith("/") or ".." in name.split("/") or "\\" in name or "\x00" in name:
        raise ValueError("unsafe cpio path")
    return str(PurePosixPath(name))


def parse_cpio(data):
    entries = {}
    pos = 0
    while pos + 110 <= len(data):
        if data[pos:pos + 6] != b"070701":
            raise ValueError("expected a newc initramfs")
        fields = [int(data[pos + 6 + i * 8:pos + 14 + i * 8], 16) for i in range(13)]
        pos += 110
        length = fields[11]
        if length < 1 or pos + length > len(data) or data[pos + length - 1] != 0:
            raise ValueError("invalid cpio name")
        name = data[pos:pos + length - 1].decode("utf8")
        pos = (pos + length + 3) & ~3
        size = fields[6]
        if pos + size > len(data):
            raise ValueError("truncated cpio file")
        payload = data[pos:pos + size]
        pos = (pos + size + 3) & ~3
        if name == "TRAILER!!!":
            return entries
        name = safe_name(name)
        if name in entries:
            raise ValueError("duplicate cpio path")
        entries[name] = (fields, payload)
    raise ValueError("missing cpio trailer")


def write_cpio(entries):
    out = bytearray()
    trailer = ([0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 11, 0], b"")
    for name, (original, data) in [*entries.items(), ("TRAILER!!!", trailer)]:
        path = name.encode("utf8") + b"\0"
        fields = list(original)
        fields[6] = len(data)
        fields[11] = len(path)
        out.extend(b"070701" + "".join(f"{value:08x}" for value in fields).encode("ascii") + path)
        out.extend(b"\0" * (-len(out) % 4))
        out.extend(data)
        out.extend(b"\0" * (-len(out) % 4))
    out.extend(b"\0" * (-len(out) % 512))
    return bytes(out)


def add(entries, name, data, mode):
    name = safe_name(name)
    inode = max((entry[0][0] for entry in entries.values()), default=0) + 1
    entries[name] = ([inode, mode, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(name) + 1, 0], data)


def run(args):
    paths = [Path(args.qemu), Path(args.kernel), Path(args.initramfs)]
    for path in paths:
        if not path.is_file():
            raise ValueError(f"file missing: {path}")
    tests = {}
    for value in args.test:
        name, path = value.split("=", 1)
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name) or name in tests:
            raise ValueError("test names must be unique lowercase identifiers")
        data = Path(path).read_bytes()
        machine = 62 if args.arch == "x64" else 183
        if len(data) < 4096 or data[:6] != b"\x7fELF\x02\x01" or int.from_bytes(data[18:20], "little") != machine:
            raise ValueError(f"{name}: expected a {args.arch} Linux ELF executable")
        tests[name] = data
    if not tests:
        raise ValueError("at least one test executable is required")
    entries = parse_cpio(gzip.decompress(paths[2].read_bytes()))
    busybox = next((f"/{name}" for name in ("bin/busybox", "usr/bin/busybox") if name in entries), None)
    if busybox is None:
        raise ValueError("initramfs needs busybox")
    if parse_cpio(write_cpio(entries)) != entries:
        raise ValueError("cpio round-trip changed source entries")
    add(entries, "probe", b"", 0o40700)
    for name, data in tests.items():
        add(entries, f"probe/{name}", data, 0o100755)
    codex = None
    bwrap = None
    if args.codex:
        if not args.codex_version or not re.fullmatch(r"codex-cli [0-9A-Za-z.+-]+", args.codex_version):
            raise ValueError("--codex needs an exact --codex-version")
        codex = Path(args.codex).read_bytes()
        machine = 62 if args.arch == "x64" else 183
        if codex[:6] != b"\x7fELF\x02\x01" or int.from_bytes(codex[18:20], "little") != machine:
            raise ValueError("Codex ELF architecture mismatch")
        add(entries, "probe/codex", codex, 0o100755)
        bundled = Path(args.codex).parent / "codex-resources/bwrap"
        if bundled.is_file():
            bwrap = bundled.read_bytes()
            add(entries, "probe/codex-resources", b"", 0o40755)
            add(entries, "probe/codex-resources/bwrap", bwrap, 0o100755)
    commands = [
        "#!/bin/busybox sh", "export PATH=/bin:/sbin:/usr/bin:/usr/sbin HOME=/root TMPDIR=/tmp",
        "/bin/busybox mkdir -p /proc /sys /dev /tmp /root",
        "/bin/busybox mount -t proc proc /proc || exit 90",
        "/bin/busybox mount -t sysfs sysfs /sys || exit 91",
        "/bin/busybox mount -t devtmpfs devtmpfs /dev || true",
        "/bin/busybox mount -t tmpfs tmpfs /tmp || exit 92",
        "echo EASY_SWITCH_TESTS_BEGIN", "overall=0",
    ]
    if codex:
        commands += ["/bin/busybox mkdir -p /probe/artifacts",
                     "export EASY_SWITCH_TEST_CODEX=/probe/codex EASY_SWITCH_TEST_ARTIFACTS=/probe/artifacts",
                     f"export EASY_SWITCH_TEST_CODEX_VERSION='{args.codex_version}'",
                     "/probe/codex --version"]
    test_filter = " --ignored real_appserver_search_keeps_appended_model_projection" if codex else ""
    for name in tests:
        commands += [f"/probe/{name} --test-threads=1 --nocapture{test_filter}", "rc=$?", f"echo TEST_RESULT {name} $rc", "[ $rc -eq 0 ] || overall=1"]
    if codex:
        commands += ["/bin/busybox cat /probe/artifacts/vscode-appserver-projection.log"]
    commands += ["echo EASY_SWITCH_TESTS_DONE $overall", "/bin/busybox sync", "/bin/busybox poweroff -f"]
    commands = [command.replace("/bin/busybox", busybox) for command in commands]
    if codex:
        # bubblewrap cannot pivot_root from initramfs; boot into a RAM-only tmpfs root.
        add(entries, "run-tests", ("\n".join(commands) + "\n").encode(), 0o100755)
        boot = ["#!/bin/busybox sh", "/bin/busybox mkdir /newroot",
                "/bin/busybox mount -t tmpfs tmpfs /newroot || exit 93",
                'for entry in /bin /sbin /usr /lib /lib64 /etc /probe /run-tests; do',
                '  if [ -e "$entry" ]; then /bin/busybox cp -a "$entry" /newroot/ || exit 94; fi',
                "done", "exec /bin/busybox switch_root /newroot /run-tests"]
        add(entries, "init", ("\n".join(boot).replace("/bin/busybox", busybox) + "\n").encode(), 0o100755)
    else:
        add(entries, "init", ("\n".join(commands) + "\n").encode(), 0o100755)
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    initrd = output / "tests-initramfs.gz"
    initrd.write_bytes(gzip.compress(write_cpio(entries), compresslevel=1 if codex else 9, mtime=0))
    machine = "q35,accel=tcg" if args.arch == "x64" else "virt,accel=tcg"
    console = "ttyS0" if args.arch == "x64" else "ttyAMA0"
    command = [str(paths[0].resolve()), "-machine", machine, "-cpu", "max", "-smp", "2", "-m", "2048" if codex else "512",
               "-nodefaults", "-nic", "none", "-display", "none", "-monitor", "none", "-serial", "stdio", "-no-reboot",
               "-kernel", str(paths[1].resolve()), "-initrd", str(initrd), "-append", f"console={console} quiet panic=-1"]
    timed_out = False
    with (output / "serial.log").open("wb") as log:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                                   creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        try:
            code = process.wait(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            process.kill()
            code = process.wait()
            timed_out = True
    text = (output / "serial.log").read_text(encoding="utf8", errors="replace")
    results = {name: int(code) for name, code in re.findall(r"^TEST_RESULT ([a-z0-9_]+) (\d+)\s*$", text, re.MULTILINE)}
    report = {
        "environment": f"QEMU {args.arch} Linux RAM-only; no NIC, disk or host share",
        "tests": results, "qemuExitCode": code, "timedOut": timed_out,
        "passed": not timed_out and code == 0 and set(results) == set(tests) and all(value == 0 for value in results.values()) and "EASY_SWITCH_TESTS_DONE 0" in text,
        "testSha256": {name: hashlib.sha256(data).hexdigest() for name, data in tests.items()},
        "kernelSha256": hashlib.sha256(paths[1].read_bytes()).hexdigest(),
        "remoteSshTested": False, "realAccountsUsed": False,
    }
    if codex:
        report["codexSha256"] = hashlib.sha256(codex).hexdigest()
        report["bundledBwrapSha256"] = hashlib.sha256(bwrap).hexdigest() if bwrap else None
        report["expectedCodexVersion"] = args.codex_version
        report["passed"] = report["passed"] and "PASS isolated real schema/parser/reconciliation" in text and "1 passed" in text
    (output / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    print(json.dumps(report, indent=2))
    if not report["passed"]:
        print(text[-10000:])
        raise SystemExit(1)


class ArchiveTests(unittest.TestCase):
    def test_roundtrip_binary_and_modes(self):
        entries = {}
        add(entries, "bin", b"", 0o40755)
        add(entries, "bin/test", bytes(range(256)), 0o100755)
        self.assertEqual(parse_cpio(write_cpio(entries)), entries)

    def test_reject_bad_paths_and_truncation(self):
        for name in ["../bad", "/absolute", "dir/../bad", "dir\\bad"]:
            with self.assertRaises(ValueError):
                safe_name(name)
        with self.assertRaises(ValueError):
            parse_cpio(b"070701")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--arch", choices=["x64", "arm64"], default="x64")
    parser.add_argument("--qemu")
    parser.add_argument("--kernel")
    parser.add_argument("--initramfs")
    parser.add_argument("--test", action="append", default=[])
    parser.add_argument("--output")
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--codex", help="Run the isolated app-server integration test with this Linux binary")
    parser.add_argument("--codex-version", help="Exact expected codex-cli version output")
    options = parser.parse_args()
    if options.self_test:
        unittest.main(argv=[__file__])
    else:
        if not all([options.qemu, options.kernel, options.initramfs, options.output]):
            parser.error("--qemu, --kernel, --initramfs and --output are required")
        run(options)
