import { execFile } from 'node:child_process';

export async function auditProtectedProcess(pid: number, home: string, sqliteHome?: string): Promise<{ pid: number; ppid: number; startTime: string }> {
  const output = await new Promise<string>((resolve, reject) => {
    const child = execFile('/usr/bin/sudo', ['-n', '/usr/bin/python3', '-I', '-B', '/usr/local/lib/easy-switch/proc-audit.py'],
      { encoding: 'utf8', timeout: 10_000, maxBuffer: 16_384, env: { PATH: '/usr/bin:/bin', LANG: 'C' }, windowsHide: true },
      (error, stdout) => {
        if (error) reject(new Error(`PID ${pid} 需要只读管理员检查。请先按安装说明安装检查器；未停止进程或修改聊天。`));
        else resolve(stdout);
      });
    child.stdin?.on('error', () => {});
    child.stdin?.end(JSON.stringify({ protocolVersion: 1, pid, home, sqliteHome }));
  });
  let result: { protocolVersion?: number; pid?: number; status?: string; ppid?: number; startTime?: string };
  try { result = JSON.parse(output); } catch { throw new Error('只读管理员检查返回无效结果，停止切换。'); }
  if (result?.protocolVersion !== 1 || result.pid !== pid || result.status !== 'ok' || !Number.isSafeInteger(result.ppid) || typeof result.startTime !== 'string' || !/^\d+$/.test(result.startTime)) throw new Error(`PID ${pid} 仍可能写入 Codex 数据，或无法完成身份检查，停止切换。`);
  return { pid, ppid: result.ppid!, startTime: result.startTime };
}
