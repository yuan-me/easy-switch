const test = require('node:test');
const assert = require('node:assert/strict');
const childProcess = require('node:child_process');
const { auditProtectedProcess } = require('../out/procAudit');

test('admin check uses only a fixed isolated read-only script and fails closed on invalid replies', async () => {
  const original = childProcess.execFile;
  let response = { protocolVersion: 1, pid: 3133, status: 'ok', ppid: 1, startTime: '1234' };
  let failure;
  childProcess.execFile = (file, args, options, callback) => {
    assert.equal(file, '/usr/bin/sudo');
    assert.deepEqual(args, ['-n', '/usr/bin/python3', '-I', '-B', '/usr/local/lib/easy-switch/proc-audit.py']);
    assert.deepEqual(options.env, { PATH: '/usr/bin:/bin', LANG: 'C' });
    assert.equal(options.timeout, 10000);
    return { stdin: { on() {}, end(input) {
      assert.deepEqual(JSON.parse(input), { protocolVersion: 1, pid: 3133, home: '/home/user/.codex', sqliteHome: '/database' });
      setImmediate(() => callback(failure, JSON.stringify(response), 'private stderr must never be shown'));
    } } };
  };
  const run = () => auditProtectedProcess(3133, '/home/user/.codex', '/database');
  try {
    assert.deepEqual(await run(), { pid: 3133, ppid: 1, startTime: '1234' });
    for (const bad of [{}, null, { ...response, status: 'busy' }, { ...response, pid: 1 }, { ...response, protocolVersion: 2 }]) {
      const saved = response; response = bad; await assert.rejects(run()); response = saved;
    }
    failure = Error('private sudo stderr');
    await assert.rejects(run(), e => e.message.includes('安装检查器') && !e.message.includes('private'));
  } finally { childProcess.execFile = original; }
});
