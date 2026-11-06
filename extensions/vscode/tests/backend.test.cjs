const test = require('node:test');
const assert = require('node:assert/strict');
const { parseStat, processHome, sameIdentity, selectBackend, supportedCodexVersions } = require('../out/backend');
const own = { pid: 30, ppid: 20, startTime: '1234', uid: 1000, exe: '/ext/bin/linux-x86_64/codex', args: ['codex', '-c', 'foo=true', 'app-server'], home: '/home/User/.codex' };

test('other protected processes require successful admin audit and stable PID before exclusion', async () => {
  const fs = require('node:fs/promises');
  const audit = require('../out/procAudit');
  const { codexProcesses } = require('../out/backend');
  const saved = { readdir: fs.readdir, readFile: fs.readFile, readlink: fs.readlink, audit: audit.auditProtectedProcess };
  let start = '1234', deny = false, calls = 0;
  fs.readdir = async () => ['3133'];
  fs.readFile = async file => file.endsWith('status') ? 'Name:\tsystemd\nUid:\t1000\t1000\t1000\t1000\n' : `3133 (systemd) S 1 ${Array(17).fill('0').join(' ')} ${start}`;
  fs.readlink = async () => { throw Object.assign(Error('protected'), { code: 'EACCES' }); };
  audit.auditProtectedProcess = async (pid, home, sqlite) => {
    calls++; assert.equal(pid, 3133); assert.equal(home, '/home/user/.codex'); assert.equal(sqlite, '/database');
    if (deny) throw Error('管理员检查拒绝');
    return { pid, ppid: 1, startTime: '1234' };
  };
  try {
    const run = () => codexProcesses(1000, '/proc', '/home/user/.codex', '/database');
    assert.deepEqual(await run(), []); assert.equal(calls, 1);
    start = '1235'; await assert.rejects(run(), /发生变化/);
    start = '1234'; deny = true; await assert.rejects(run(), /管理员检查拒绝/);
  } finally { fs.readdir = saved.readdir; fs.readFile = saved.readFile; fs.readlink = saved.readlink; audit.auditProtectedProcess = saved.audit; }
});

test('protected SSH transport is exempt only with a stable root monitor; unknown denied processes block', async () => {
  const fs = require('node:fs/promises');
  const path = require('node:path');
  const { codexProcesses } = require('../out/backend');
  const originals = { readdir: fs.readdir, readFile: fs.readFile, readlink: fs.readlink, lstat: fs.lstat };
  const stat = (pid, ppid, start = '1234') => `${pid} (sshd-session) S ${ppid} ${Array(17).fill('0').join(' ')} ${start}`;
  const status = (name, uid) => `Name:\t${name}\nUid:\t${uid}\t${uid}\t${uid}\t${uid}\n`;
  const root = 'mock-proc', files = new Map();
  const put = (pid, name, value) => files.set(path.join(root, String(pid), name), value);
  put(100, 'status', status('sshd-session', 1000)); put(100, 'stat', stat(100, 90));
  put(100, 'cmdline', 'sshd-session: guiyn@notty\0');
  put(90, 'status', status('sshd-session', 0)); put(90, 'stat', stat(90, 1));
  put(90, 'cmdline', 'sshd-session: guiyn [priv]\0');
  put(30, 'status', status('codex', 1000)); put(30, 'stat', stat(30, 20));
  put(30, 'cmdline', 'codex\0app-server\0'); put(30, 'environ', 'HOME=/home/guiyn\0');
  let rootOwner = 0, reused = false, childReads = 0, gone = false;
  fs.readdir = async () => ['100', '90', '30'];
  fs.readFile = async file => {
    if (file === path.join(root, '100', 'stat') && reused && ++childReads > 1) return stat(100, 90, '9999');
    assert.ok(files.has(file), `unexpected read: ${file}`); return files.get(file);
  };
  fs.readlink = async file => {
    if (file === path.join(root, '100', 'exe')) throw Object.assign(Error('protected'), { code: gone ? 'ENOENT' : 'EACCES' });
    assert.equal(file, path.join(root, '30', 'exe')); return '/opt/codex';
  };
  fs.lstat = async file => ({ uid: rootOwner, mode: 0o700, isSymbolicLink: () => path.basename(file) === 'exe', isDirectory: () => path.basename(file) === 'fd' });
  try {
    assert.deepEqual((await codexProcesses(1000, root)).map(p => p.pid), [30]);
    for (const [pid, name, bad] of [
      [90, 'status', status('sshd-session', 1000)], [90, 'cmdline', 'sshd-session: other [priv]\0'],
      [100, 'status', status('codex', 1000)], [100, 'cmdline', 'sshd-session: guiyn@notty\0extra'],
      [100, 'stat', stat(100, 1)],
    ]) {
      const old = files.get(path.join(root, String(pid), name)); put(pid, name, bad);
      await assert.rejects(codexProcesses(1000, root), /PID 100/); put(pid, name, old);
    }
    rootOwner = 1000; await assert.rejects(codexProcesses(1000, root), /PID 100/); rootOwner = 0;
    reused = true; await assert.rejects(codexProcesses(1000, root), /PID 100/); reused = false;
    gone = true; assert.deepEqual((await codexProcesses(1000, root)).map(p => p.pid), [30]);
  } finally { Object.assign(fs, originals); }
});
test('proc stat parser handles spaces and parentheses in comm', () => {
  const fields = ['S', '20', ...Array(17).fill('0'), '1234', '0'];
  assert.deepEqual(parseStat(`30 (codex app (worker)) ${fields.join(' ')}`), { pid: 30, ppid: 20, startTime: '1234' });
});
test('process Home uses CODEX_HOME before HOME without exposing any credentials', () => {
  assert.equal(processHome('HOME=/home/User\0CODEX_HOME=/srv/Codex\0SECRET=never-return\0'), '/srv/Codex');
  assert.equal(processHome('HOME=/home/User\0'), '/home/User/.codex');
  assert.equal(processHome('HOME=relative\0'), undefined);
});
test('only unique same-host child is selectable; other writers block', () => {
  assert.equal(selectBackend([own], own.home, own.exe, 20), own);
  assert.throws(() => selectBackend([own, { ...own, pid: 31, ppid: 21 }], own.home, own.exe, 20), /其他或无法归属/);
  assert.throws(() => selectBackend([{ ...own, ppid: 21, home: undefined }], own.home, own.exe, 20), /其他或无法归属/);
  assert.throws(() => selectBackend([own, { ...own, pid: 31 }], own.home, own.exe, 20), /多个/);
});
test('current window backend with a different or unknown Home rejects before selection', () => {
  for (const home of ['/home/user/.codex', undefined]) {
    assert.throws(() => selectBackend([{ ...own, home }], own.home, own.exe, 20), /数据目录与识别目标不一致/);
  }
  const otherWindow = { ...own, pid: 31, ppid: 21, home: '/home/user/.codex' };
  assert.equal(selectBackend([otherWindow], own.home, own.exe, 20), undefined);
  assert.equal(selectBackend([own, otherWindow], own.home, own.exe, 20), own);
});
test('PID reuse and executable replacement fail identity check', () => {
  assert.equal(sameIdentity(own, { ...own }), true);
  assert.equal(sameIdentity(own, { ...own, startTime: '1235' }), false);
  assert.equal(sameIdentity(own, { ...own, exe: own.exe + ' (deleted)' }), false);
  assert.equal(sameIdentity(own, { ...own, args: ['codex', 'exec'] }), false);
  assert.ok(supportedCodexVersions.has('26.1021.22027'));
  assert.ok(!supportedCodexVersions.has('99.1.0'));
});
