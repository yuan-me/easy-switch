import { createHash, randomBytes, randomUUID } from 'node:crypto';

export interface Provider { id: string; name: string; baseUrl: string; model: string }
export const official: Provider = { id: 'official', name: '官方 ChatGPT', baseUrl: '', model: '' };
export interface State { get<T>(key: string): T | undefined; update(key: string, value: unknown): Thenable<void> | Promise<void> }
export interface Secrets { get(key: string): Thenable<string | undefined> | Promise<string | undefined>; store(key: string, value: string): Thenable<void> | Promise<void>; delete(key: string): Thenable<void> | Promise<void> }
export function homeId(home: string): string { return createHash('sha256').update(home).digest('hex'); }
export function validateOfficialModel(text: string): string | undefined {
  return !text.trim() ? '请输入官方模型 ID' : text.trim().length > 200 || /\p{Cc}/u.test(text) ? '模型 ID 不能超过 200 个字符或包含控制字符' : undefined;
}

export function validateProvider(value: Provider): void {
  if (!/^esv_[0-9a-f]{32}$/.test(value.id)) throw new Error('服务商 ID 无效。');
  if (!value.name.trim() || !value.model.trim()) throw new Error('名称和模型不能为空。');
  if (value.name.length > 200 || value.model.length > 200 || /\p{Cc}/u.test(value.name + value.model)) throw new Error('名称或模型包含无效字符，或超过长度限制。');
  let url: URL;
  try { url = new URL(value.baseUrl); } catch { throw new Error('请输入完整的 HTTP/HTTPS API 地址。'); }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw new Error('API 地址只支持 HTTP/HTTPS，不能包含用户名、密码、查询参数或片段。');
}

export class ProviderStore {
  private constructor(private readonly state: State, private readonly secrets: Secrets, readonly namespace: string, readonly scope: string) {}
  static async open(state: State, secrets: Secrets, home: string): Promise<ProviderStore> {
    let namespace = state.get<string>('namespace');
    if (!namespace) { namespace = randomUUID(); await state.update('namespace', namespace); }
    if (!/^[0-9a-f-]{36}$/.test(namespace)) throw new Error('插件环境标识损坏，请保留现有数据并检查扩展存储。');
    return new ProviderStore(state, secrets, namespace, homeId(home));
  }
  private get listKey(): string { return `providers:${this.scope}`; }
  officialModel(): string | undefined {
    const value = this.state.get<unknown>(`officialModel:${this.scope}`);
    if (value === undefined) return undefined;
    if (typeof value !== 'string' || validateOfficialModel(value)) throw new Error('保存的官方模型 ID 无效，请检查扩展存储。');
    return value;
  }
  async saveOfficialModel(value: string): Promise<void> {
    const error = validateOfficialModel(value);
    if (error) throw new Error(error);
    await this.state.update(`officialModel:${this.scope}`, value.trim());
  }
  private secretKey(id: string): string { return `${this.namespace}:${this.scope}:${id}`; }
  list(): Provider[] {
    const values = this.state.get<unknown>(this.listKey) ?? [];
    if (!Array.isArray(values)) throw new Error('服务商列表损坏，未覆盖现有配置。');
    const result = values.map(value => {
      if (!value || typeof value !== 'object') throw new Error('服务商列表包含无效记录。');
      const p = value as Provider;
      if (typeof p.id !== 'string' || typeof p.name !== 'string' || typeof p.baseUrl !== 'string' || typeof p.model !== 'string') throw new Error('服务商记录格式无效。');
      validateProvider(p);
      return { id: p.id, name: p.name, baseUrl: p.baseUrl, model: p.model };
    });
    if (new Set(result.map(p => p.id)).size !== result.length) throw new Error('服务商列表包含重复 ID。');
    return result;
  }
  key(id: string): PromiseLike<string | undefined> { return this.secrets.get(this.secretKey(`provider:${id}`)); }
  async save(provider: Provider, key?: string): Promise<void> {
    validateProvider(provider);
    const previous = await this.key(provider.id);
    if (!key && !previous) throw new Error('请输入 API 密钥。');
    if (key && (key.length > 16384 || !key.trim() || /\p{Cc}/u.test(key))) throw new Error('API 密钥格式无效。');
    const secret = this.secretKey(`provider:${provider.id}`);
    if (key) await this.secrets.store(secret, key);
    try {
      // Reread after asynchronous credential work so another window's edits are retained.
      const list = this.list();
      await this.state.update(this.listKey, [...list.filter(p => p.id !== provider.id), provider]);
    } catch {
      if (key) { if (previous) await this.secrets.store(secret, previous); else await this.secrets.delete(secret); }
      throw new Error('服务商保存失败，原配置已保留。');
    }
  }
  async remove(id: string): Promise<void> {
    const list = this.list();
    await this.state.update(this.listKey, list.filter(p => p.id !== id));
    try { await this.secrets.delete(this.secretKey(`provider:${id}`)); }
    catch { await this.state.update(this.listKey, list); throw new Error('密钥删除失败，服务商记录已恢复。'); }
  }
  async journalKey(create = true): Promise<string> {
    const name = this.secretKey('journal');
    const existing = await this.secrets.get(name);
    if (existing) {
      if (!/^[A-Za-z0-9+/]{43}=$/.test(existing) || Buffer.from(existing, 'base64').length !== 32) throw new Error('操作恢复密钥损坏，未修改服务器数据。');
      return existing;
    }
    if (!create) throw new Error('当前客户端缺少操作恢复密钥。请使用发起切换时的 VS Code 客户端恢复，服务器上的备份会保留。');
    const created = randomBytes(32).toString('base64');
    await this.secrets.store(name, created);
    return created;
  }
}
