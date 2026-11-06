const test = require('node:test');
const assert = require('node:assert/strict');
const { parseShellEnvironment, discoverTarget } = require('../out/environment');
test('login-shell paths survive banners and spaces and preserve Linux case', () => {
  assert.deepEqual(parseShellEnvironment('banner\n__EASY_SWITCH_ENV_BEGIN__\n/home/A User/.codex\n/data/sqlite\n__EASY_SWITCH_ENV_END__\n'), { home: '/home/A User/.codex', sqliteHome: '/data/sqlite' });
  assert.deepEqual(parseShellEnvironment('__EASY_SWITCH_ENV_BEGIN__\n/home/a/.codex\n\n__EASY_SWITCH_ENV_END__'), { home: '/home/a/.codex' });
});
test('failed, relative, or Windows shell output does not fall back to another Home', async () => {
  assert.throws(() => parseShellEnvironment('garbage'), /无法识别/);
  assert.throws(() => parseShellEnvironment('__EASY_SWITCH_ENV_BEGIN__\nrelative\n\n__EASY_SWITCH_ENV_END__'), /绝对路径/);
  await assert.rejects(discoverTarget({ CODEX_HOME: 'C:\\Users\\x\\.codex' }), /绝对路径/);
  await assert.rejects(discoverTarget({ SHELL: '/usr/bin/fish', HOME: '/home/user' }), /明确设置/);
});
test('explicit remote environment does not launch a login shell', async () => {
  assert.deepEqual(await discoverTarget({ CODEX_HOME: '/srv/codex', CODEX_SQLITE_HOME: '/srv/database', SHELL: '/nonexistent' }), { home: '/srv/codex', sqliteHome: '/srv/database' });
});
