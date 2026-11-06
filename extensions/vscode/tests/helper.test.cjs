const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { runHelper } = require('../out/helper');
const request = { protocolVersion: 1, action: 'inspect', home: '/synthetic', storeRoot: '/synthetic/store', apiKey: 'test-api-secret', journalKey: 'test-journal-secret', officialModel: 'official-model' };
async function fixture(callback) {
  const before = process.env.NODE_OPTIONS;
  process.env.NODE_OPTIONS = `--require ${JSON.stringify(path.join(__dirname, 'fixtures', 'helper.cjs'))}`;
  try { return await callback(); }
  finally { if (before === undefined) delete process.env.NODE_OPTIONS; else process.env.NODE_OPTIONS = before; }
}
test('helper secrets are sent through stdin with no command-line arguments; progress is redacted', () => fixture(async () => {
  const progress = [];
  const result = await runHelper(process.execPath, request, item => progress.push(item));
  assert.equal(result.hasKey, true);
  assert.deepEqual(result.childArgs, []);
  assert.equal(result.officialModel, request.officialModel);
  assert.equal(progress.length, 1);
  assert.ok(!progress[0].detail.includes(request.apiKey));
}));
test('structured helper errors cannot expose supplied secrets', () => fixture(async () => {
  await assert.rejects(runHelper(process.execPath, { ...request, action: 'switch' }), error => {
    assert.ok(!error.message.includes(request.apiKey));
    assert.ok(!error.message.includes(request.journalKey));
    assert.match(error.message, /已隐藏/);
    return true;
  });
}));
test('missing protocol version is rejected instead of showing a false success', () => fixture(async () => {
  await assert.rejects(runHelper(process.execPath, { ...request, operationId: 'malformed' }), /无法识别/);
}));
