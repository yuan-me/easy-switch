import importlib.util
import io
import json
import types
from contextlib import redirect_stdout
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('audit', Path(__file__).with_name('proc-audit.py'))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='.proc-audit-test-', dir=Path.cwd())
        self.addCleanup(self.tmp.cleanup)
        self.proc = Path(self.tmp.name)
        self.root = self.proc / '100'
        (self.root / 'fd').mkdir(parents=True)
        (self.root / 'fdinfo').mkdir()
        (self.root / 'status').write_text('Name:\tsystemd\nUid:\t1000\t1000\t1000\t1000\n')
        (self.root / 'stat').write_text('100 (systemd) S 1 ' + '0 ' * 17 + '1234')
        (self.root / 'cmdline').write_bytes(b'/usr/lib/systemd/systemd\0--user\0')
        self.exe = '/usr/lib/systemd/systemd'
        self.fd = '/data/Codex/state.db'
        self.request = {'protocolVersion': 1, 'pid': 100, 'home': '/data/Codex', 'sqliteHome': '/database'}
        self.links = patch.object(audit.os, 'readlink', side_effect=lambda path: self.exe if Path(path).name == 'exe' else self.fd)
        self.links.start()
        self.addCleanup(self.links.stop)

    def check(self):
        return audit.inspect(self.proc, 1000, self.request)

    def test_system_services_are_checked_without_name_exemptions(self):
        for name in ('systemd', '(sd-pam)', 'ssh-agent', 'vmtoolsd'):
            self.exe = '/usr/bin/' + name
            self.assertEqual(self.check()['status'], 'ok')

    def test_codex_including_deleted_binary_is_blocked(self):
        for name in ('codex', 'codex-x86_64', 'codex.bin', 'codex (deleted)'):
            self.exe = '/opt/' + name
            self.assertEqual(self.check()['status'], 'busy')

    def test_writable_home_and_independent_database_are_blocked(self):
        (self.root / 'fd' / '3').touch()
        (self.root / 'fdinfo' / '3').write_text('flags:\t0100002\n')
        for target in ('/data/Codex/state.db', '/database/state.sqlite-wal'):
            self.fd = target
            self.assertEqual(self.check()['status'], 'busy')
        self.fd = '/data/Codex-other/state.db'
        self.assertEqual(self.check()['status'], 'ok')
        self.fd = '/data/Codex/state.db'
        (self.root / 'fdinfo' / '3').write_text('flags:\t0100000\n')
        self.assertEqual(self.check()['status'], 'ok')

    def test_pid_reuse_and_credentials_change_are_rejected(self):
        with patch.object(audit, 'stamp', side_effect=[(100, 1, '1234'), (100, 1, '1235')]):
            self.assertEqual(self.check()['status'], 'changed')
        (self.root / 'status').write_text('Uid:\t0\t0\t0\t0\n')
        self.assertEqual(self.check()['status'], 'denied')

    def test_unreadable_or_incomplete_process_never_passes(self):
        with patch.object(audit.os, 'readlink', side_effect=PermissionError):
            with self.assertRaises(PermissionError):
                self.check()
        (self.root / 'cmdline').unlink()
        self.assertEqual(self.check()['status'], 'denied')

    def test_invalid_requests_are_rejected_and_files_unchanged(self):
        before = {str(p): p.read_bytes() for p in self.root.rglob('*') if p.is_file()}
        self.assertEqual(self.check()['status'], 'ok')
        self.assertEqual(before, {str(p): p.read_bytes() for p in self.root.rglob('*') if p.is_file()})
        for key, value in [('pid', '../1'), ('pid', True), ('home', '/'), ('home', '/data/../root'), ('protocolVersion', 2)]:
            with self.assertRaises(ValueError):
                audit.inspect(self.proc, 1000, dict(self.request, **{key: value}))
        with self.assertRaises(ValueError):
            audit.inspect(self.proc, 0, self.request)

    def test_cli_requires_sudo_and_binds_audit_to_sudo_uid(self):
        real_inspect = audit.inspect
        for effective, owner, expected in [(1000, '1000', None), (0, '0', None), (0, '1001', 'denied'), (0, '1000', 'ok')]:
            output = io.StringIO()
            with patch.object(audit.os, 'geteuid', return_value=effective, create=True), patch.object(audit.signal, 'alarm', create=True), patch.dict(audit.os.environ, {'SUDO_UID': owner}), patch.object(audit.sys, 'argv', ['proc-audit.py']), patch.object(audit.sys, 'stdin', types.SimpleNamespace(buffer=io.BytesIO(json.dumps(self.request).encode()))), patch.object(audit, 'inspect', side_effect=lambda proc, uid, request: real_inspect(self.proc, uid, request)), redirect_stdout(output):
                if expected is None:
                    with self.assertRaises(ValueError): audit.main()
                else:
                    audit.main()
                    self.assertEqual(json.loads(output.getvalue())['status'], expected)


if __name__ == '__main__':
    unittest.main()
