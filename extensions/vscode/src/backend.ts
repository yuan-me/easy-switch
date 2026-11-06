import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { auditProtectedProcess } from './procAudit';

// Exact releases checked against their bundled Linux app-server; see COMPATIBILITY.md.
export const supportedCodexVersions = new Set(['26.1021.22027', '26.1007.21434', '26.1002.51308', '26.51021.22027']);
export interface ProcessIdentity { pid: number; ppid: number; startTime: string; uid: number; exe: string; args: string[]; home?: string }
export function parseStat(text: string): Pick<ProcessIdentity, 'pid' | 'ppid' | 'startTime'> {
  const close = text.lastIndexOf(')');
  const fields = text.slice(close + 2).trim().split(/\s+/);
  const pid = Number(text.slice(0, text.indexOf(' ')));
  const ppid = Number(fields[1]);
  const startTime = fields[19];
  if (close < 0 || !Number.isSafeInteger(pid) || pid < 1 || !Number.isSafeInteger(ppid) || !startTime || !/^\d+$/.test(startTime)) throw new Error('无法校验服务器进程身份。');
  return { pid, ppid, startTime };
}
export function processHome(environ: string): string | undefined {
  const vars = new Map(environ.split('\0').filter(Boolean).map(entry => { const i = entry.indexOf('='); return [entry.slice(0, i), entry.slice(i + 1)]; }));
  const value = vars.get('CODEX_HOME') || (vars.get('HOME') ? path.posix.join(vars.get('HOME')!, '.codex') : undefined);
  return value && path.posix.isAbsolute(value) && !/[\r\n\0]/.test(value) ? path.posix.normalize(value) : undefined;
}
export function sameIdentity(a: ProcessIdentity, b: ProcessIdentity): boolean {
  return a.pid === b.pid && a.ppid === b.ppid && a.startTime === b.startTime && a.uid === b.uid && a.exe === b.exe && a.home === b.home && a.args.length === b.args.length && a.args.every((arg, i) => arg === b.args[i]);
}
const vanished = (error: unknown) => ['ENOENT', 'ESRCH'].includes((error as NodeJS.ErrnoException).code ?? '');

// OpenSSH's post-auth transport is non-dumpable despite running as the login user.
export async function protectedSshSession(pid: number, uid: number, procRoot = '/proc'): Promise<boolean> {
  const dir = path.join(procRoot, String(pid));
  const statusIs = (text: string, owner: number) => /^Name:\s+sshd-session$/m.test(text)
    && new RegExp(`^Uid:\\s+${owner}\\s+${owner}\\s+${owner}\\s+${owner}\\s*$`, 'm').test(text);
  const title = async (root: string) => (await fs.readFile(path.join(root, 'cmdline'), 'utf8')).replace(/\0+$/, '');
  try {
    if (uid === 0 || !statusIs(await fs.readFile(path.join(dir, 'status'), 'utf8'), uid)) return false;
    const child = parseStat(await fs.readFile(path.join(dir, 'stat'), 'utf8'));
    const command = await title(dir);
    const match = /^sshd-session: ([a-zA-Z0-9_.-]+)@(notty|pts\/\d+)$/.exec(command);
    if (!match || child.pid !== pid || child.ppid <= 1) return false;
    const parent = path.join(procRoot, String(child.ppid));
    const parentStat = parseStat(await fs.readFile(path.join(parent, 'stat'), 'utf8'));
    const parentTitle = `sshd-session: ${match[1]} [priv]`;
    if (parentStat.pid !== child.ppid || !statusIs(await fs.readFile(path.join(parent, 'status'), 'utf8'), 0) || await title(parent) !== parentTitle) return false;
    const [exe, fd] = await Promise.all([fs.lstat(path.join(dir, 'exe')), fs.lstat(path.join(dir, 'fd'))]);
    if (!exe.isSymbolicLink() || exe.uid !== 0 || !fd.isDirectory() || fd.uid !== 0 || (fd.mode & 0o077) !== 0) return false;
    const after = parseStat(await fs.readFile(path.join(dir, 'stat'), 'utf8'));
    const parentAfter = parseStat(await fs.readFile(path.join(parent, 'stat'), 'utf8'));
    return after.pid === pid && after.ppid === child.ppid && after.startTime === child.startTime
      && parentAfter.pid === parentStat.pid && parentAfter.startTime === parentStat.startTime
      && statusIs(await fs.readFile(path.join(dir, 'status'), 'utf8'), uid)
      && statusIs(await fs.readFile(path.join(parent, 'status'), 'utf8'), 0)
      && await title(dir) === command && await title(parent) === parentTitle;
  } catch { return false; }
}

export async function readProcess(pid: number, procRoot = '/proc'): Promise<ProcessIdentity | undefined> {
  const root = path.join(procRoot, String(pid));
  try {
    const [stat, status, exe, command, environ] = await Promise.all([fs.readFile(path.join(root, 'stat'), 'utf8'), fs.readFile(path.join(root, 'status'), 'utf8'), fs.readlink(path.join(root, 'exe')), fs.readFile(path.join(root, 'cmdline'), 'utf8'), fs.readFile(path.join(root, 'environ'), 'utf8')]);
    const uid = Number(/^Uid:\s+(\d+)/m.exec(status)?.[1]);
    if (!Number.isSafeInteger(uid)) throw new Error('无法校验 Codex 进程所属用户。');
    return { ...parseStat(stat), uid, exe, args: command.split('\0').filter(Boolean), home: processHome(environ) };
  } catch (error) { if (vanished(error)) return undefined; throw new Error(`无法读取 PID ${pid} 的身份，未发送停止信号。`); }
}

export async function codexProcesses(uid: number, procRoot = '/proc', home?: string, sqliteHome?: string): Promise<ProcessIdentity[]> {
  const found: ProcessIdentity[] = [];
  for (const name of await fs.readdir(procRoot)) {
    if (!/^\d+$/.test(name)) continue;
    const dir = path.join(procRoot, name);
    try {
      const status = await fs.readFile(path.join(dir, 'status'), 'utf8');
      if (Number(/^Uid:\s+(\d+)/m.exec(status)?.[1]) !== uid) continue;
      const exe = await fs.readlink(path.join(dir, 'exe')).catch(async error => {
        if (['EACCES', 'EPERM'].includes(error.code) && await protectedSshSession(Number(name), uid, procRoot)) return undefined;
        if (['EACCES', 'EPERM'].includes(error.code) && home && procRoot === '/proc') {
          const checked = await auditProtectedProcess(Number(name), home, sqliteHome);
          const current = parseStat(await fs.readFile(path.join(dir, 'stat'), 'utf8'));
          if (current.pid !== checked.pid || current.ppid !== checked.ppid || current.startTime !== checked.startTime) throw new Error(`PID ${name} 在管理员检查后发生变化，请刷新重试。`);
          return undefined;
        }
        throw error;
      });
      if (exe === undefined) continue;
      if (!/^codex(?:$|[-.])/.test(path.posix.basename(exe))) continue;
      const item = await readProcess(Number(name), procRoot);
      if (item) found.push(item);
    } catch (error) {
      if (vanished(error)) continue;
      // A same-user process hidden by procfs cannot safely be assumed unrelated.
      throw new Error(`无法检查服务器 PID ${name}，无法确认 Codex 已停止写入。${error instanceof Error && !('code' in error) ? error.message : ''}`);
    }
  }
  return found;
}
export function selectBackend(all: ProcessIdentity[], home: string, executable: string, parentPid: number): ProcessIdentity | undefined {
  const own = all.filter(item => item.ppid === parentPid && item.exe === executable && item.args.includes('app-server'));
  if (own.some(item => item.home !== home)) throw new Error('当前 Codex 后端数据目录与识别目标不一致，请核对环境后重试。');
  const related = all.filter(item => item.home === home || item.home === undefined);
  const other = related.filter(item => !own.includes(item));
  if (other.length) throw new Error(`同一 Codex 目录仍有其他或无法归属的进程（PID ${other.map(p => p.pid).join('、')}）。请先结束其他 SSH 窗口或 CLI 中的 Codex，再重试。`);
  if (own.length > 1) throw new Error('当前窗口存在多个 Codex 后端，无法唯一确认停止目标。');
  return own[0];
}
export async function stopBackend(home: string, executable: string, parentPid = process.pid, sqliteHome?: string): Promise<boolean> {
  if (process.platform !== 'linux' || !process.getuid) throw new Error('仅支持 Linux Remote-SSH。');
  const uid = process.getuid();
  const realExecutable = await fs.realpath(executable);
  const selected = selectBackend(await codexProcesses(uid, '/proc', home, sqliteHome), home, realExecutable, parentPid);
  if (!selected) return false;
  const current = await readProcess(selected.pid);
  if (!current || !sameIdentity(selected, current)) throw new Error('Codex 进程在检查后发生变化，请刷新重试。');
  try { process.kill(current.pid, 'SIGTERM'); }
  catch (error) { if (!vanished(error)) throw new Error('无法正常停止当前 Codex 后端，未执行迁移。'); }
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const alive = await readProcess(current.pid);
    if (!alive || alive.startTime !== current.startTime) {
      const remaining = (await codexProcesses(uid, '/proc', home, sqliteHome)).filter(item => item.home === home || !item.home);
      if (remaining.length) throw new Error('Codex 停止后出现新的写入进程，请关闭其他客户端后重试。');
      return true;
    }
    await delay(150);
  }
  throw new Error('Codex 后端未在 15 秒内正常退出；未强制结束进程，未执行迁移。');
}
