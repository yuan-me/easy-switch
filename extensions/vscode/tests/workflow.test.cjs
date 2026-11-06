const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const fs = require('node:fs/promises');
const helper = require('../out/helper');
const backend = require('../out/backend');
const environment = require('../out/environment');
const { ProviderStore } = require('../out/storage');

const provider = { id: 'esv_' + 'a'.repeat(32), name: '服务商 A', baseUrl: 'https://example.com/v1', model: 'model-a' };
const home = '/home/test/.codex';
const pending = [{ id: 'operation-one', description: '中断切换', state: 'Applying', fileCount: 3 }];

async function host(options, check) {
  const values = new Map(), secrets = new Map(), commands = new Map(), events = [], messages = [], inputs = [];
  const state = { get: key => values.get(key), update: async (key, value) => { values.set(key, structuredClone(value)); } };
  const secretStorage = { get: async key => secrets.get(key), store: async (key, value) => { secrets.set(key, value); }, delete: async key => { secrets.delete(key); } };
  const store = await ProviderStore.open(state, secretStorage, home);
  await store.save(provider, 'test-key');
  let view, phase = options.pending ? 'pending' : 'ready', activated = 0, inspections = 0;
  const snapshot = () => ({ type: 'result', protocolVersion: 1, home, sqliteHome: options.removeSqliteConfig && inspections > 1 ? options.envSqlite ?? null : '/data/sqlite', activeProviderId: 'official', officialModel: options.officialModel === undefined ? 'official-model' : options.officialModel, configFingerprint: phase === 'pending' ? null : 'fingerprint', sessionCount: phase === 'pending' ? null : 2, operations: phase === 'pending' ? pending : [] });
  const codex = { extensionKind: 2, extensionPath: '/extensions/openai.chatgpt', packageJSON: { version: options.version ?? '26.1021.22027' }, activate: async () => { activated++; } };
  class Disposable { dispose() {} }
  const vscode = {
    TreeItem: class { constructor(label) { this.label = label; } }, TreeItemCollapsibleState: { None: 0 },
    ThemeIcon: class { constructor(id) { this.id = id; } }, ThemeColor: class {},
    EventEmitter: class extends Disposable { event = () => new Disposable(); fire() {} },
    StatusBarAlignment: { Left: 1 }, ProgressLocation: { Notification: 15 }, ExtensionKind: { Workspace: 2 },
    env: { remoteName: 'ssh-remote' },
    extensions: { getExtension: () => codex },
    workspace: { workspaceFolders: [{ uri: { fsPath: '/project' } }], getConfiguration: () => ({ get: () => undefined }) },
    commands: {
      registerCommand: (name, callback) => { commands.set(name, callback); return new Disposable(); },
      executeCommand: async name => { if (name === 'chatgpt.openSidebar') events.push('openCodex'); if (name === 'workbench.action.reloadWindow') { events.push('reload'); if (options.reloadFails) throw Error('reload unavailable'); } }
    },
    window: {
      createStatusBarItem: () => ({ show() {}, dispose() {} }),
      registerTreeDataProvider: (_, provider) => { view = provider; return new Disposable(); },
      showWarningMessage: async (message) => { messages.push(message); return message.startsWith('切换到') ? '切换并重载' : undefined; },
      showErrorMessage: async (message, ...actions) => { messages.push(message); events.push({ errorActions: actions }); },
      showInformationMessage: async message => { messages.push(message); },
      showQuickPick: async items => items.find(item => item.provider?.id === provider.id),
      showInputBox: async input => { inputs.push(input); events.push('modelInput'); return options.modelInput; },
      withProgress: async (_, callback) => callback({ report() {} })
    }
  };
  const saved = { load: Module._load, access: fs.access, run: helper.runHelper, stop: backend.stopBackend, discover: environment.discoverTarget, platform: Object.getOwnPropertyDescriptor(process, 'platform') };
  Module._load = function(name, ...args) { return name === 'vscode' ? vscode : saved.load.call(this, name, ...args); };
  fs.access = async () => {};
  environment.discoverTarget = async () => ({ home, ...(options.envSqlite ? { sqliteHome: options.envSqlite } : {}) });
  backend.stopBackend = async () => { events.push('stop'); if (options.stopFails) throw Error('无法检查服务器 PID 3133'); return true; };
  helper.runHelper = async (_, request) => {
    if (request.action === 'inspect') { inspections++; events.push({ inspect: structuredClone(request) }); return snapshot(); }
    events.push({ mutation: structuredClone(request) });
    if (options.mutationFails) { phase = 'pending'; throw Error('模拟部分提交'); }
    phase = 'ready';
    return { type: 'result', protocolVersion: 1, committed: true, activeProviderId: provider.id };
  };
  Object.defineProperty(process, 'platform', { value: 'linux' });
  try {
    delete require.cache[require.resolve('../out/extension')];
    const { activate } = require('../out/extension');
    const context = { extension: { extensionKind: 2 }, extensionPath: '/extension', globalStorageUri: { fsPath: '/plugin-storage' }, globalState: state, secrets: secretStorage, subscriptions: [] };
    await activate(context);
    await check({ commands, events, messages, inputs, view, workspace: vscode.workspace, get activated() { return activated; } });
  } finally {
    Module._load = saved.load; fs.access = saved.access; helper.runHelper = saved.run; backend.stopBackend = saved.stop; environment.discoverTarget = saved.discover;
    Object.defineProperty(process, 'platform', saved.platform);
    delete require.cache[require.resolve('../out/extension')];
  }
}

test('target validation precedes backend stop and displays inspected directory without inventing env override', () => host({}, async ({ commands, events, view }) => {
  assert.ok(view.getChildren().some(row => String(row.tooltip).includes('/data/sqlite')));
  await commands.get('easySwitch.switchProvider')({ provider });
  const targetIndex = events.findIndex(e => e.inspect?.provider);
  const stopIndex = events.indexOf('stop');
  assert.ok(targetIndex >= 0 && targetIndex < stopIndex);
  assert.equal(events[targetIndex].inspect.apiKey, 'test-key');
  const mutation = events.find(e => e.mutation)?.mutation;
  assert.equal(mutation.expectedConfigFingerprint, 'fingerprint');
  assert.equal(mutation.officialModel, 'official-model');
  assert.ok(!events.includes('modelInput'));
  assert.equal(mutation.sqliteHome, undefined);
  assert.deepEqual(mutation.projectDirs, ['/project']);
  assert.equal(events.at(-1), 'reload');
}));
test('unknown official model can be cancelled before preflight or backend stop', () => host({ officialModel: null }, async ({ commands, events, inputs }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.equal(inputs.length, 1);
  assert.equal(inputs[0].prompt, '切回官方时使用的模型 ID（在 Codex 模型选择器查看）');
  assert.ok(!events.includes('stop'));
  assert.ok(!events.some(e => e.inspect?.provider || e.mutation));
}));

test('official model confirmation persists across failed stop and refresh, without a second prompt', () => host({ officialModel: null, modelInput: 'official-model-id', stopFails: true }, async ({ commands, events, inputs }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  await commands.get('easySwitch.refresh')();
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.equal(inputs.length, 1);
  assert.equal(events.filter(e => e === 'stop').length, 2);
  assert.ok(!events.some(e => e.mutation));
  assert.ok(events.filter(e => e.inspect?.provider).every(e => e.inspect.officialModel === 'official-model-id'));
}));

test('official row edits fallback model without overriding the server configured model', () => host({ modelInput: 'fallback-model' }, async ({ commands, events, inputs }) => {
  await commands.get('easySwitch.editProvider')({ provider: { id: 'official' } });
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.equal(inputs.length, 1);
  assert.equal(events.find(e => e.mutation).mutation.officialModel, 'official-model');
}));
test('official model input is validated before stop and included in preflight and mutation', () => host({ officialModel: null, modelInput: '  official-model-id  ' }, async ({ commands, events, inputs }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  const validation = inputs[0].validateInput;
  assert.ok(validation(''));
  assert.ok(validation('   '));
  assert.ok(validation('a'.repeat(201)));
  assert.ok(validation('model\n'));
  assert.ok(validation('model\u0085'));
  assert.equal(validation('a'.repeat(200)), undefined);
  assert.equal(events.find(e => e.inspect?.provider).inspect.officialModel, 'official-model-id');
  assert.equal(events.find(e => e.mutation).mutation.officialModel, 'official-model-id');
  assert.ok(events.indexOf('modelInput') < events.indexOf('stop'));
}));
test('invalid official model returned by input cannot reach backend stop', () => host({ officialModel: null, modelInput: 'bad\nmodel' }, async ({ commands, events, messages }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.ok(messages.some(message => message.includes('控制字符')));
  assert.ok(!events.includes('stop'));
  assert.ok(!events.some(e => e.mutation));
}));
test('removing sqlite_home from config does not leave a stale effective directory in later requests', () => host({ removeSqliteConfig: true }, async ({ commands, events }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.ok(events.filter(e => e.inspect).every(e => e.inspect.sqliteHome === undefined));
  assert.equal(events.find(e => e.mutation).mutation.sqliteHome, undefined);
}));
test('real environment sqlite directory remains separate from an overriding config directory', () => host({ removeSqliteConfig: true, envSqlite: '/environment-db' }, async ({ commands, events }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.ok(events.filter(e => e.inspect).every(e => e.inspect.sqliteHome === '/environment-db'));
  assert.equal(events.find(e => e.mutation).mutation.sqliteHome, '/environment-db');
}));
test('workspace folders added after refresh are checked in preflight and mutation requests', () => host({}, async ({ commands, events, workspace }) => {
  workspace.workspaceFolders.push({ uri: { fsPath: '/new-project' } });
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.deepEqual(events.find(e => e.inspect?.provider).inspect.projectDirs, ['/project', '/new-project']);
  assert.deepEqual(events.find(e => e.mutation).mutation.projectDirs, ['/project', '/new-project']);
}));
test('partial commit refreshes recovery state and blocks automatic and manual reload', () => host({ mutationFails: true }, async ({ commands, events, messages, view }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.equal(events.filter(e => e.inspect).length, 3);
  assert.ok(view.getChildren().some(row => String(row.label).includes('待恢复')));
  assert.ok(!events.includes('reload'));
  assert.deepEqual(events.find(e => e.errorActions)?.errorActions, []);
  await commands.get('easySwitch.reload')();
  await commands.get('easySwitch.openCodex')();
  assert.ok(messages.some(message => message.includes('请先刷新并恢复中断操作')));
  assert.ok(!events.includes('reload'));
  assert.ok(!events.includes('openCodex'));
}));
test('startup with interrupted operation does not activate Codex or offer a normal switch', () => host({ pending: true }, async ({ commands, events, activated, view }) => {
  assert.equal(activated, 0);
  assert.ok(view.getChildren().some(row => String(row.label).includes('待恢复')));
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.ok(!events.includes('stop'));
  assert.ok(!events.some(e => e.mutation));
}));
for (const version of ['26.1021.22027', '26.1007.21434', '26.1002.51308', '26.51021.22027']) {
  test(`validated Codex ${version} completes preflight, stop, mutation and reload`, () => host({ version }, async ({ commands, events, messages }) => {
    await commands.get('easySwitch.switchProvider')({ provider });
    const stop = events.indexOf('stop'), mutation = events.findIndex(e => e.mutation), reload = events.indexOf('reload');
    assert.ok(stop >= 0 && mutation > stop && reload > mutation, JSON.stringify({ events, messages }));
    assert.equal(events.filter(e => e.mutation).length, 1);
  }));
}
test('unknown official version is rejected before stopping backend', () => host({ version: '99.0.0' }, async ({ commands, events, messages }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.ok(messages.some(message => message.includes('尚未通过兼容测试')));
  assert.ok(!events.includes('stop'));
  assert.ok(!events.some(e => e.mutation));
}));
test('reload failure reports committed configuration and does not run another mutation', () => host({ reloadFails: true }, async ({ commands, events, messages }) => {
  await commands.get('easySwitch.switchProvider')({ provider });
  assert.ok(messages.some(message => message.includes('配置已保存，但窗口未能重载')));
  assert.equal(events.filter(e => e.mutation).length, 1);
}));
