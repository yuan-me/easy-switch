import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import { randomUUID } from 'node:crypto';
import { absolutePath, discoverTarget, type Target } from './environment';
import { stopBackend, supportedCodexVersions } from './backend';
import { runHelper, type HelperRequest, type HelperResult, type Operation } from './helper';
import { ProviderStore, official, validateProvider, validateOfficialModel, type Provider } from './storage';

class Row extends vscode.TreeItem {
  constructor(label: string, readonly provider?: Provider) {
    super(label, vscode.TreeItemCollapsibleState.None);
    if (provider) {
      this.id = provider.id;
      this.contextValue = provider.id === 'official' ? 'official-provider' : 'api-provider';
      this.iconPath = new vscode.ThemeIcon(provider.id === 'official' ? 'account' : 'server');
      this.command = { command: 'easySwitch.switchProvider', title: '切换并重载', arguments: [this] };
      this.tooltip = provider.id === 'official' ? '使用服务器上保留的 ChatGPT 登录。' : `${provider.name}\n${provider.baseUrl}\n模型：${provider.model}\n点击“切换并重载”后使用此服务商。`;
    }
  }
}
interface Ready { sourceTarget: Target; target: Target; store: ProviderStore; storeRoot: string; binary: string; codex: vscode.Extension<unknown> }
const unfinished = (operation: Operation) => !['Complete', 'Restored', 'RolledBack'].includes(operation.state);

class SwitchView implements vscode.TreeDataProvider<Row>, vscode.Disposable {
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  private readonly status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 20);
  private ready?: Ready;
  private result?: HelperResult;
  private error?: string;
  private progress?: string;
  private busy = false;
  private stopped = false;
  private reloadPending = false;
  private operationUncertain = false;
  private promptedRecovery = false;
  constructor(private readonly context: vscode.ExtensionContext) {
    this.status.command = 'easySwitch.switchProvider';
    this.status.text = '$(arrow-swap) Easy Switch';
    this.status.show();
  }
  dispose(): void { this.status.dispose(); this.changed.dispose(); }
  getTreeItem(item: Row): vscode.TreeItem { return item; }
  getChildren(): Row[] {
    if (this.error) { const row = new Row(this.error); row.iconPath = new vscode.ThemeIcon('warning'); row.tooltip = this.error; return [row]; }
    if (!this.ready || !this.result) return [new Row('正在检查服务器配置…')];
    const rows = [new Row(`SSH · ${os.hostname()}`), new Row(this.ready.target.home)];
    rows[0]!.iconPath = new vscode.ThemeIcon('remote');
    rows[1]!.iconPath = new vscode.ThemeIcon('folder');
    rows[1]!.tooltip = `Codex 数据目录：${this.ready.target.home}\n数据库：${this.ready.target.sqliteHome ?? `${this.ready.target.home}/sqlite`}\n使用此目录的其他 Codex 窗口和 CLI 也会使用这份配置。`;
    if (this.progress) { const p = new Row(this.progress); p.iconPath = new vscode.ThemeIcon('loading~spin'); rows.push(p); }
    if ((this.reloadPending || this.stopped) && !this.operationUncertain && !this.result.operations?.some(unfinished)) {
      const reload = new Row(this.reloadPending ? '配置已保存，点击重载窗口' : 'Codex 已停止，点击重载窗口');
      reload.iconPath = new vscode.ThemeIcon('refresh');
      reload.command = { command: 'easySwitch.reload', title: '重载窗口' };
      rows.push(reload);
    }
    for (const operation of this.result.operations?.filter(unfinished) ?? []) {
      const row = new Row(`待恢复：${operation.description}`);
      row.iconPath = new vscode.ThemeIcon('warning');
      row.command = { command: 'easySwitch.recover', title: '恢复中断操作' };
      rows.push(row);
    }
    for (const provider of [official, ...this.ready.store.list()]) {
      const row = new Row(provider.name, provider);
      row.description = provider.id === this.result.activeProviderId ? '当前配置' : provider.id === 'official' ? '' : provider.model;
      if (provider.id === this.result.activeProviderId) row.iconPath = new vscode.ThemeIcon('check', new vscode.ThemeColor('testing.iconPassed'));
      rows.push(row);
    }
    if (!this.ready.store.list().length) rows.push(new Row('点击上方 + 添加 API 服务商'));
    return rows;
  }
  private update(): void {
    const active = this.result?.activeProviderId;
    const name = active === 'official' ? official.name : this.ready?.store.list().find(p => p.id === active)?.name ?? '外部配置';
    this.status.text = this.busy ? '$(sync~spin) Easy Switch' : `$(arrow-swap) ${this.reloadPending ? '待重载' : name}`;
    this.status.tooltip = this.error ?? `Easy Switch · ${os.hostname()}\n配置目录：${this.ready?.target.home ?? '尚未识别'}\n显示当前保存的配置；连接是否正常，请发送一条消息确认。`;
    this.changed.fire();
  }
  async command(task: () => Promise<void>): Promise<void> {
    if (this.busy) { void vscode.window.showInformationMessage('切换操作正在进行，请等待完成。'); return; }
    this.busy = true;
    await vscode.commands.executeCommand('setContext', 'easySwitch.busy', true);
    try { await task(); }
    catch (error) {
      if (this.operationUncertain && this.ready) {
        try {
          this.result = await runHelper(this.ready.binary, this.request(this.ready, 'inspect'));
          this.adoptTarget(this.ready, this.result);
          this.operationUncertain = false;
        } catch { /* Keep reload blocked until the operation state can be inspected. */ }
      }
      const message = error instanceof Error ? error.message : '操作失败，请刷新检查状态。';
      const canReload = this.stopped && !this.operationUncertain && !this.result?.operations?.some(unfinished);
      const action = await vscode.window.showErrorMessage(message, ...(canReload ? ['重载 Codex'] : []));
      if (action === '重载 Codex') await this.reload();
    } finally {
      this.busy = false; this.progress = undefined;
      await vscode.commands.executeCommand('setContext', 'easySwitch.busy', false);
      this.update();
    }
  }
  private async resolve(): Promise<Ready> {
    if (process.platform !== 'linux' || vscode.env.remoteName !== 'ssh-remote') throw new Error('请在连接 Linux 服务器的 Remote-SSH 窗口中，将 Easy Switch 安装到 SSH 服务器。');
    if (this.context.extension.extensionKind !== vscode.ExtensionKind.Workspace) throw new Error('Easy Switch 当前没有运行在服务器端，请检查远程扩展安装位置。');
    const codex = vscode.extensions.getExtension('openai.chatgpt');
    if (!codex || codex.extensionKind !== vscode.ExtensionKind.Workspace) throw new Error('请在此 SSH 服务器上安装并启用官方 Codex（openai.chatgpt）。');
    const target = await discoverTarget();
    const store = await ProviderStore.open(this.context.globalState, this.context.secrets, target.home);
    const arch = process.arch === 'x64' ? 'linux-x64' : process.arch === 'arm64' ? 'linux-arm64' : undefined;
    if (!arch) throw new Error('当前服务器架构不受支持。请使用 Linux x64 或 arm64。');
    const binary = path.join(this.context.extensionPath, 'bin', arch, 'easy-switch-helper');
    await fs.access(binary, fs.constants.X_OK).catch(() => { throw new Error('切换工具缺失或无法执行，请重新安装 Easy Switch。'); });
    const storeRoot = path.join(this.context.globalStorageUri.fsPath, store.namespace, store.scope);
    return { sourceTarget: { ...target }, target, store, storeRoot, binary, codex };
  }
  private request(ready: Ready, action: HelperRequest['action']): HelperRequest {
    // Effective paths from config must not become an environment override on the next request.
    const projectDirs = (vscode.workspace.workspaceFolders ?? []).map(folder => folder.uri.fsPath);
    return { protocolVersion: 1, action, ...ready.sourceTarget, storeRoot: ready.storeRoot, projectDirs };
  }
  private adoptTarget(ready: Ready, result: HelperResult): void {
    if (result.home !== ready.target.home) throw new Error('切换工具返回的 Codex Home 与当前目标不一致，已停止操作。');
    if (result.sqliteHome != null) ready.target.sqliteHome = absolutePath(result.sqliteHome, '数据库目录');
    else delete ready.target.sqliteHome;
  }
  async refresh(): Promise<void> {
    try {
      const ready = await this.resolve();
      const result = await runHelper(ready.binary, this.request(ready, 'inspect'));
      this.adoptTarget(ready, result);
      this.ready = ready; this.result = result; this.error = undefined; this.operationUncertain = false;
      this.update();
      if (!this.promptedRecovery && result.operations?.some(unfinished)) {
        this.promptedRecovery = true;
        void vscode.window.showWarningMessage('上次切换未完成。请点击侧边栏的“待恢复”项目，恢复后再切换。');
      }
    } catch (error) {
      this.ready = undefined; this.result = undefined;
      this.error = error instanceof Error ? error.message : '无法检查服务器配置。';
      this.update();
      throw error;
    }
  }
  private requireReady(): Ready { if (!this.ready || !this.result) throw new Error(this.error ?? '请先刷新并确认服务器配置。'); return this.ready; }
  private async choose(row?: Row, apiOnly = false): Promise<Provider | undefined> {
    const ready = this.requireReady();
    const values = [...(apiOnly ? [] : [official]), ...ready.store.list()];
    if (row?.provider) {
      const found = values.find(p => p.id === row.provider!.id);
      if (!found) throw new Error('该服务商已经被删除，请刷新。');
      return found;
    }
    const item = await vscode.window.showQuickPick(values.map(p => ({ label: p.name, description: p.id === 'official' ? '使用已有 ChatGPT 登录' : p.model, provider: p })), { title: 'Easy Switch · 选择服务商', placeHolder: '选择要操作的服务商' });
    return item?.provider;
  }
  async edit(row?: Row, create = false): Promise<void> {
    const ready = this.requireReady();
    if (!create && row?.provider?.id === 'official') { await this.configureOfficialModel(ready); return; }
    const existing = create ? undefined : await this.choose(row, true);
    if (!create && !existing) return;
    const options = { ignoreFocusOut: true, title: existing ? '编辑服务商' : '添加服务商' };
    const name = await vscode.window.showInputBox({ ...options, prompt: '服务商名称', value: existing?.name ?? '', validateInput: text => text.trim() ? undefined : '请输入名称' });
    if (name === undefined) return;
    const baseUrl = await vscode.window.showInputBox({ ...options, prompt: 'API 地址，例如 https://example.com/v1（需支持 Responses，不含 /responses）', value: existing?.baseUrl ?? '' });
    if (baseUrl === undefined) return;
    const model = await vscode.window.showInputBox({ ...options, prompt: '模型 ID（使用服务商提供的模型名称）', value: existing?.model ?? '', validateInput: text => text.trim() ? undefined : '请输入模型 ID' });
    if (model === undefined) return;
    const provider: Provider = { id: existing?.id ?? `esv_${randomUUID().replaceAll('-', '')}`, name: name.trim(), baseUrl: baseUrl.trim().replace(/\/+$/, ''), model: model.trim() };
    validateProvider(provider);
    const savedKey = existing ? await ready.store.key(existing.id) : undefined;
    const key = await vscode.window.showInputBox({ ...options, password: true, prompt: savedKey ? 'API 密钥（留空则不修改）' : 'API 密钥', placeHolder: savedKey ? '已保存密钥' : undefined });
    if (key === undefined) return;
    if (provider.baseUrl.startsWith('http:') && !['localhost', '127.0.0.1', '[::1]'].includes(new URL(provider.baseUrl).hostname)) {
      const choice = await vscode.window.showWarningMessage('这个 API 地址使用未加密的 HTTP，发送请求时密钥和聊天内容可能被网络中的其他人读取。', { modal: true }, '仍然保存');
      if (!choice) return;
    }
    await ready.store.save(provider, key || undefined);
    this.update();
    void vscode.window.showInformationMessage('已保存服务商。点击“切换并重载”开始使用。');
  }
  async remove(row?: Row): Promise<void> {
    const ready = this.requireReady();
    const provider = await this.choose(row, true);
    if (!provider) return;
    if (provider.id === this.result?.activeProviderId) throw new Error('请先切换到其他服务商，再删除当前服务商。');
    const action = await vscode.window.showWarningMessage(`删除“${provider.name}”及插件保存的密钥？`, { modal: true, detail: '此操作不会删除 Codex 聊天。' }, '删除');
    if (!action) return;
    await ready.store.remove(provider.id); this.update();
  }
  private supported(ready: Ready): string {
    const version: unknown = ready.codex.packageJSON.version;
    if (typeof version !== 'string' || !supportedCodexVersions.has(version)) throw new Error(`当前服务器上的 Codex 版本（${typeof version === 'string' ? version : '未知'}）尚未通过兼容测试。请使用以下版本：${[...supportedCodexVersions].join('、')}。未修改任何配置或聊天。`);
    if (vscode.workspace.getConfiguration('chatgpt').get<string>('cliExecutable')) throw new Error('当前 Codex 使用自定义 CLI，无法确认后端归属。请恢复官方内置 CLI 后重试。');
    return path.join(ready.codex.extensionPath, 'bin', process.arch === 'arm64' ? 'linux-aarch64' : 'linux-x86_64', 'codex');
  }
  async switch(row?: Row): Promise<void> {
    const provider = await this.choose(row);
    if (!provider) return;
    const ready = this.requireReady();
    const executable = this.supported(ready);
    const apiKey = provider.id === 'official' ? undefined : await ready.store.key(provider.id);
    if (provider.id !== 'official' && !apiKey) throw new Error('当前客户端没有该服务商的密钥，请编辑服务商重新输入。');
    if (this.result?.operations?.some(unfinished)) throw new Error('存在未完成的切换，请先运行“恢复中断的切换”。');
    let officialModel = this.result?.officialModel ?? ready.store.officialModel();
    if (officialModel == null) {
      officialModel = await this.configureOfficialModel(ready);
      if (officialModel === undefined) return;
    }
    const before = await runHelper(ready.binary, { ...this.request(ready, 'inspect'), provider: provider.id === 'official' ? { id: 'official' } : provider, apiKey, officialModel });
    this.adoptTarget(ready, before);
    this.result = before;
    if (before.operations?.some(unfinished)) throw new Error('存在未完成的切换，请先运行“恢复中断的切换”。');
    const configFingerprint = before.configFingerprint;
    if (!configFingerprint) throw new Error('切换工具没有返回配置指纹，无法安全提交。');
    officialModel = before.officialModel ?? officialModel;
    const accepted = await vscode.window.showWarningMessage(`切换到“${provider.name}”并重载 VS Code？`, { modal: true, detail: `服务器：${os.hostname()}\nCodex 数据目录：${ready.target.home}\n已有聊天：${before.sessionCount ?? '未知'} 个\n\n请先结束正在生成的任务。切换会停止当前窗口的 Codex、迁移已有聊天并更新共享配置。API 密钥会以明文保存在服务器配置中。如果新服务商无法读取加密历史，原记录会保留，可切回原服务商。` }, '切换并重载');
    if (!accepted) return;
    const journalKey = await ready.store.journalKey();
    await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: 'Easy Switch', cancellable: false }, async progress => {
      this.progress = '正在停止 Codex'; this.update(); progress.report({ message: this.progress });
      this.stopped = await stopBackend(ready.target.home, executable, process.pid, ready.target.sqliteHome) || this.stopped;
      this.operationUncertain = true;
      const result = await runHelper(ready.binary, { ...this.request(ready, 'switch'), provider: provider.id === 'official' ? { id: 'official' } : provider, apiKey, journalKey, expectedConfigFingerprint: configFingerprint, officialModel }, event => {
        this.progress = `${event.stage}：${event.detail}`; this.update(); progress.report({ message: this.progress });
      });
      if (!result.committed) throw new Error('切换结果尚未确认。请刷新并检查是否有待恢复的操作。');
      this.result = result; this.operationUncertain = false; this.reloadPending = true; this.update();
    });
    await this.reload();
  }
  private async configureOfficialModel(ready: Ready): Promise<string | undefined> {
    const input = await vscode.window.showInputBox({ title: 'Easy Switch · 官方恢复模型', prompt: '切回官方时使用的模型 ID（在 Codex 模型选择器查看）', placeHolder: '填写一次即可保存；官方配置已指定模型时优先使用配置中的值', value: ready.store.officialModel(), ignoreFocusOut: true, validateInput: validateOfficialModel });
    if (input === undefined) return undefined;
    await ready.store.saveOfficialModel(input);
    return input.trim();
  }
  async recover(): Promise<void> {
    const ready = this.requireReady();
    const executable = this.supported(ready);
    const snapshot = await runHelper(ready.binary, this.request(ready, 'inspect'));
    this.adoptTarget(ready, snapshot);
    const pending = snapshot.operations?.filter(unfinished) ?? [];
    if (!pending.length) { this.result = snapshot; void vscode.window.showInformationMessage('没有需要恢复的操作。'); return; }
    const choice = pending.length === 1 ? pending[0] : (await vscode.window.showQuickPick(pending.map(operation => ({ label: operation.description, description: operation.state, operation })), { title: '选择要恢复的操作' }))?.operation;
    if (!choice) return;
    const accepted = await vscode.window.showWarningMessage('恢复中断前的配置和会话，然后重载窗口？', { modal: true, detail: '请先结束正在生成的任务。恢复会校验备份和现有文件；发生外部修改时将停止，不覆盖新的内容。' }, '恢复并重载');
    if (!accepted) return;
    const journalKey = await ready.store.journalKey(false);
    this.stopped = await stopBackend(ready.target.home, executable, process.pid, ready.target.sqliteHome) || this.stopped;
    this.operationUncertain = true;
    const result = await runHelper(ready.binary, { ...this.request(ready, 'recover'), operationId: choice.id, journalKey }, event => { this.progress = `${event.stage}：${event.detail}`; this.update(); });
    if (!result.committed) throw new Error('恢复结果尚未确认。请刷新并检查待恢复的操作。');
    this.result = result; this.operationUncertain = false; this.reloadPending = true; this.update();
    await this.reload();
  }
  private ensureStable(): void {
    this.requireReady();
    if (this.operationUncertain || this.result?.operations?.some(unfinished)) throw new Error('切换尚未完成。请先刷新并恢复中断操作，再重载 Codex，以免写入尚未迁移完的聊天。');
  }
  async openCodex(): Promise<void> {
    this.ensureStable();
    await vscode.commands.executeCommand('chatgpt.openSidebar');
  }
  async reload(): Promise<void> {
    this.ensureStable();
    try { await vscode.commands.executeCommand('workbench.action.reloadWindow'); }
    catch { throw new Error(this.reloadPending ? '配置已保存，但窗口未能重载。请运行“开发人员: 重新加载窗口”。' : '窗口重载失败，请手动重新加载窗口。'); }
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const view = new SwitchView(context);
  context.subscriptions.push(view, vscode.window.registerTreeDataProvider('easySwitch.providers', view));
  const commands: Record<string, (row?: Row) => Promise<void>> = {
    addProvider: () => view.edit(undefined, true), editProvider: row => view.edit(row), removeProvider: row => view.remove(row),
    switchProvider: row => view.switch(row), refresh: () => view.refresh(), recover: () => view.recover(), reload: () => view.reload(),
    openCodex: () => view.openCodex()
  };
  for (const [name, callback] of Object.entries(commands)) context.subscriptions.push(vscode.commands.registerCommand(`easySwitch.${name}`, (row?: Row) => view.command(() => callback(row))));
  // Refresh errors belong in the sidebar; do not interrupt every unrelated VS Code startup.
  try { await view.refresh(); } catch { /* refresh preserves the actionable error in the sidebar */ }
}
