import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import * as path from 'node:path';
const run = promisify(execFile);
const begin = '__EASY_SWITCH_ENV_BEGIN__';
const end = '__EASY_SWITCH_ENV_END__';
export interface Target { home: string; sqliteHome?: string }
export function absolutePath(value: string, name: string): string {
  if (!value || !path.posix.isAbsolute(value) || /[\0\r\n]/.test(value)) throw new Error(`${name} 不是有效的 Linux 绝对路径。`);
  return path.posix.normalize(value);
}
export function parseShellEnvironment(output: string): Target {
  const lines = output.split(/\r?\n/);
  const i = lines.lastIndexOf(begin);
  if (i < 0 || lines[i + 3] !== end) throw new Error('无法识别服务器登录 Shell 返回的 Codex 目录，未回退到本机路径。');
  const home = absolutePath(lines[i + 1] ?? '', 'CODEX_HOME');
  const sqlite = lines[i + 2];
  return { home, ...(sqlite ? { sqliteHome: absolutePath(sqlite, 'CODEX_SQLITE_HOME') } : {}) };
}
export async function discoverTarget(env: NodeJS.ProcessEnv = process.env): Promise<Target> {
  if (env.CODEX_HOME) return { home: absolutePath(env.CODEX_HOME, 'CODEX_HOME'), ...(env.CODEX_SQLITE_HOME ? { sqliteHome: absolutePath(env.CODEX_SQLITE_HOME, 'CODEX_SQLITE_HOME') } : {}) };
  const shell = env.SHELL;
  if (!shell || !path.posix.isAbsolute(shell) || !['bash', 'sh', 'dash', 'zsh', 'ksh'].includes(path.posix.basename(shell))) throw new Error('无法可靠解析服务器登录环境。请在启动 VS Code Server 的环境中明确设置 CODEX_HOME。');
  const script = `printf '\\n%s\\n%s\\n%s\\n%s\\n' '${begin}' "\${CODEX_HOME:-$HOME/.codex}" "\${CODEX_SQLITE_HOME:-}" '${end}'`;
  try {
    const { stdout } = await run(shell, ['-l', '-i', '-c', script], { env, timeout: 10_000, maxBuffer: 64 * 1024, encoding: 'utf8' });
    return parseShellEnvironment(stdout);
  } catch { throw new Error('服务器登录环境读取失败或超时。请明确设置 CODEX_HOME 后重连 SSH；未修改任何配置。'); }
}
