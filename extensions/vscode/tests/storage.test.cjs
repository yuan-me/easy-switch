const test = require('node:test');
const assert = require('node:assert/strict');
const { ProviderStore, validateProvider } = require('../out/storage');
class State {
  values = new Map(); fail = false;
  get(key) { return this.values.get(key); }
  async update(key, value) { if (this.fail) throw Error('simulated'); this.values.set(key, structuredClone(value)); }
}
class Secrets {
  values = new Map();
  async get(key) { return this.values.get(key); }
  async store(key, value) { this.values.set(key, value); }
  async delete(key) { this.values.delete(key); }
}
const provider = { id: 'esv_' + 'a'.repeat(32), name: '服务商 A', baseUrl: 'https://example.com/v1', model: 'model-a' };

test('official fallback model survives reopening and stays isolated by server and Home', async () => {
  const state = new State(), secrets = new Secrets();
  const store = await ProviderStore.open(state, secrets, '/home/user/.codex');
  await store.saveOfficialModel('  official-model  ');
  assert.equal((await ProviderStore.open(state, secrets, '/home/user/.codex')).officialModel(), 'official-model');
  assert.equal((await ProviderStore.open(new State(), secrets, '/home/user/.codex')).officialModel(), undefined);
  assert.equal((await ProviderStore.open(state, secrets, '/other/.codex')).officialModel(), undefined);
  for (const bad of ['', 'bad\nmodel', 'a'.repeat(201)]) await assert.rejects(store.saveOfficialModel(bad));
  assert.equal(store.officialModel(), 'official-model');
  state.fail = true;
  await assert.rejects(store.saveOfficialModel('changed'));
  assert.equal(store.officialModel(), 'official-model');
});
test('provider metadata never contains a secret; edits keep saved key and journal key is stable', async () => {
  const state = new State(), secrets = new Secrets();
  const store = await ProviderStore.open(state, secrets, '/home/user/.codex');
  await store.save(provider, 'test-secret');
  await store.save({ ...provider, name: '新名称' });
  assert.equal(await store.key(provider.id), 'test-secret');
  assert.equal(store.list()[0].name, '新名称');
  assert.ok(!JSON.stringify([...state.values]).includes('test-secret'));
  const key = await store.journalKey();
  assert.equal(Buffer.from(key, 'base64').length, 32);
  assert.equal(await (await ProviderStore.open(state, secrets, '/home/user/.codex')).journalKey(false), key);
  await store.remove(provider.id);
  assert.equal(await store.key(provider.id), undefined);
  assert.equal(store.list().length, 0);
});
test('same Home on different hosts and different Homes on same host do not share secrets', async () => {
  const state = new State(), secrets = new Secrets();
  const a = await ProviderStore.open(state, secrets, '/home/user/.codex');
  const b = await ProviderStore.open(new State(), secrets, '/home/user/.codex');
  const c = await ProviderStore.open(state, secrets, '/home/user/.codex-other');
  await a.save(provider, 'server-a');
  assert.equal(await b.key(provider.id), undefined);
  assert.equal(await c.key(provider.id), undefined);
  await assert.rejects(b.journalKey(false), /客户端缺少/);
});
test('failed metadata save restores previous key', async () => {
  const state = new State(), secrets = new Secrets();
  const store = await ProviderStore.open(state, secrets, '/home/user/.codex');
  await store.save(provider, 'before');
  state.fail = true;
  await assert.rejects(store.save({ ...provider, name: 'changed' }, 'after'), /保存失败/);
  assert.equal(await store.key(provider.id), 'before');
  assert.equal(store.list()[0].name, provider.name);
});
test('untrusted provider fields and secret input are rejected', async () => {
  assert.throws(() => validateProvider({ ...provider, baseUrl: 'https://user:secret@example.com/v1' }), /不能包含/);
  assert.throws(() => validateProvider({ ...provider, baseUrl: 'file:///tmp/config' }), /只支持/);
  assert.throws(() => validateProvider({ ...provider, id: '../../override' }), /ID/);
  const store = await ProviderStore.open(new State(), new Secrets(), '/home/user/.codex');
  await assert.rejects(store.save(provider, 'one\ntwo'), /格式无效/);
  await assert.rejects(store.save(provider, '  '), /格式无效/);
  await assert.rejects(store.save(provider, 'one\u007ftwo'), /格式无效/);
  assert.throws(() => validateProvider({ ...provider, model: 'a'.repeat(201) }), /长度限制/);
});
test('credential await does not lose another window\'s newly saved provider', async () => {
  const state = new State(), secrets = new Secrets();
  const store = await ProviderStore.open(state, secrets, '/home/user/.codex');
  const other = { ...provider, id: 'esv_' + 'b'.repeat(32), name: '并行添加' };
  const original = secrets.store.bind(secrets);
  secrets.store = async (key, value) => {
    await original(key, value);
    await state.update(`providers:${store.scope}`, [other]);
  };
  await store.save(provider, 'new-key');
  assert.deepEqual(new Set(store.list().map(p => p.id)), new Set([provider.id, other.id]));
});
