"""Privileged, read-only /proc audit. No environment, credential or application file reads."""
import json
import os
from pathlib import Path, PurePosixPath
import signal
import sys

PROTOCOL = 1


def stamp(root):
    value = (root / 'stat').read_text()
    fields = value[value.rindex(')') + 1:].split()
    return int(value.split(' ', 1)[0]), int(fields[1]), str(int(fields[19]))


def absolute(value):
    if not isinstance(value, str) or not value.startswith('/') or any(ord(c) < 32 for c in value):
        raise ValueError('invalid path')
    path = PurePosixPath('/' + value.lstrip('/'))
    if '..' in path.parts or str(path) == '/':
        raise ValueError('invalid path')
    return path


def inspect(proc, owner, request):
    if set(request) - {'protocolVersion', 'pid', 'home', 'sqliteHome'} or type(request.get('protocolVersion')) is not int or request.get('protocolVersion') != PROTOCOL:
        raise ValueError('invalid protocol')
    pid = request.get('pid')
    if type(pid) is not int or not 1 < pid < 2**31 or owner <= 0:
        raise ValueError('invalid identity')
    homes = [absolute(request.get('home'))]
    if request.get('sqliteHome') is not None:
        homes.append(absolute(request['sqliteHome']))
    root = proc / str(pid)
    reply = {'protocolVersion': PROTOCOL, 'pid': pid}
    try:
        before = stamp(root)
        ids = next(line.split()[1:] for line in (root / 'status').read_text().splitlines() if line.startswith('Uid:'))
        if len(ids) != 4 or any(int(uid) != owner for uid in ids) or before[0] != pid:
            return dict(reply, status='denied')
        exe = os.readlink(root / 'exe')
        # Read only argv[0], never command arguments or /proc/*/environ.
        with (root / 'cmdline').open('rb', buffering=0) as command:
            argv0 = bytearray()
            while len(argv0) <= 4096:
                byte = command.read(1)
                if byte in (b'', b'\0'):
                    break
                argv0.extend(byte)
            else:
                return dict(reply, status='denied')
        for name in (PurePosixPath(exe.removesuffix(' (deleted)')).name, PurePosixPath(argv0.decode('utf-8')).name):
            if name == 'codex' or name.startswith(('codex-', 'codex.')):
                return dict(reply, status='busy')
        for descriptor in (root / 'fd').iterdir():
            if not descriptor.name.isdecimal():
                return dict(reply, status='denied')
            try:
                target = PurePosixPath(os.readlink(descriptor))
                if not target.is_absolute() or not any(target.is_relative_to(home) for home in homes):
                    continue
                info = (root / 'fdinfo' / descriptor.name).read_text()
                flags = int(next(line.split()[1] for line in info.splitlines() if line.startswith('flags:')), 8)
                if flags & 3:
                    return dict(reply, status='busy')
            except FileNotFoundError:
                continue
        after_ids = next(line.split()[1:] for line in (root / 'status').read_text().splitlines() if line.startswith('Uid:'))
        if stamp(root) != before or os.readlink(root / 'exe') != exe or after_ids != ids:
            return dict(reply, status='changed')
        return dict(reply, status='ok', ppid=before[1], startTime=before[2])
    except ProcessLookupError:
        return dict(reply, status='gone')
    except FileNotFoundError:
        # A missing process file alone does not prove that the process exited.
        return dict(reply, status='gone' if not root.exists() else 'denied')


def main():
    signal.alarm(8)
    if len(sys.argv) != 1 or os.geteuid() != 0:
        raise ValueError('sudo required')
    owner = int(os.environ['SUDO_UID'])
    data = sys.stdin.buffer.read(8193)
    if len(data) > 8192:
        raise ValueError('input too large')
    result = inspect(Path('/proc'), owner, json.loads(data))
    print(json.dumps(result))


if __name__ == '__main__':
    try:
        main()
    except Exception:
        print(json.dumps({'protocolVersion': PROTOCOL, 'status': 'denied'}))
        sys.exit(1)
