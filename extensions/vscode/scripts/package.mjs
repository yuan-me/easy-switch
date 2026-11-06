import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chmod, copyFile, lstat, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { auditVsix, checkElf, helpers, setHelperPermissions } from './package-lib.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const manifest = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'));
assert.match(manifest.name, /^[a-z0-9-]+$/);
assert.match(manifest.version, /^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/);
assert.equal(Object.keys(manifest.dependencies ?? {}).length, 0, 'runtime dependencies must be bundled before using this allowlist package');
const dist = join(root, 'dist');
await mkdir(dist, { recursive: true });
const stage = await mkdtemp(join(dist, '.package-'));
const expected = new Map();
try {
  execFileSync(process.execPath, [require.resolve('typescript/bin/tsc'), '-p', root], { cwd: root, stdio: 'inherit' });
  const copy = async (name, packagedName = name) => {
    const source = join(root, name);
    assert((await lstat(source)).isFile(), `only regular files may be packaged: ${name}`);
    const dest = join(stage, packagedName);
    await mkdir(dirname(dest), { recursive: true });
    await copyFile(source, dest);
    expected.set(packagedName.replaceAll('\\', '/'), await readFile(dest));
  };
  const compiled = async (directory) => {
    const info = await lstat(join(root, directory));
    assert(info.isDirectory() && !info.isSymbolicLink(), 'compiled output must be a regular directory');
    for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
      const name = join(directory, entry.name);
      assert(!entry.isSymbolicLink(), 'symlinks may not be packaged');
      if (entry.isDirectory()) await compiled(name);
      else if (entry.name.endsWith('.js')) await copy(name);
    }
  };
  await compiled('out');
  // vsce stores the readme under this canonical lower-case ZIP path.
  await copy('README.md', 'readme.md');
  await copy('LICENSE', 'LICENSE.txt');
  await copy('VALIDATION.md');
  await copy('COMPATIBILITY.md');
  for (const name of ['compatibility-matrix.json', 'compatibility-packages.json', 'release-0.1.3-sha256.json']) {
    await copy(`scripts/validation/${name}`);
  }
  for (const arch of ['x64', 'arm64']) {
    for (const version of ['26.1002.51308', '26.1007.21434', '26.51021.22027']) {
      for (const suffix of ['txt', 'json']) await copy(`scripts/validation/compat-${arch}-${version}.${suffix}`);
    }
  }
  await copy('scripts/proc-audit.py');
  await copy('scripts/install-proc-audit.sh');
  for (const name of ['admin-audit-x64.txt', 'admin-audit-x64.json', 'admin-audit-arm64.txt', 'admin-audit-arm64.json', 'admin-audit-sha256.json']) {
    await copy(`scripts/validation/${name}`);
  }
  for (const name of ['linux-x64.txt', 'linux-x64.json', 'linux-arm64.txt', 'linux-arm64.json', 'windows-appserver-projection.txt', 'source-sha256.json', 'ssh-guard-x64.txt', 'ssh-guard-x64.json', 'ssh-guard-arm64.txt', 'ssh-guard-arm64.json', 'ssh-guard-sha256.json']) {
    await copy(`scripts/validation/${name}`);
  }
  await copy('resources/switch.svg');
  await copy('resources/icon.png');
  for (const [name, machine] of Object.entries(helpers)) {
    checkElf(await readFile(join(root, name)), machine);
    await copy(name);
    await chmod(join(stage, name), 0o755);
  }
  const { scripts, devDependencies, files, ...installedManifest } = manifest;
  const content = Buffer.from(`${JSON.stringify({ ...installedManifest, files: [...expected.keys(), 'package.json'] }, null, 2)}\n`);
  await writeFile(join(stage, 'package.json'), content);
  expected.set('package.json', content);
  const output = join(dist, `${manifest.name}-${manifest.version}.vsix`);
  const temporary = join(stage, `${manifest.name}.vsix`);
  const vsceManifestPath = require.resolve('@vscode/vsce/package.json');
  const vsceManifest = JSON.parse(await readFile(vsceManifestPath, 'utf8'));
  const cli = join(dirname(vsceManifestPath), vsceManifest.bin.vsce);
  execFileSync(process.execPath, [cli, 'package', '--no-dependencies', '--allow-missing-repository', '--no-rewrite-relative-links', '--out', temporary], { cwd: stage, stdio: 'inherit' });
  const zip = await readFile(temporary);
  setHelperPermissions(zip);
  const audit = auditVsix(zip, expected);
  await writeFile(temporary, zip);
  await rename(temporary, output);
  const report = {
    ...audit,
    file: relative(root, output).replaceAll('\\', '/'),
    sha256: createHash('sha256').update(zip).digest('hex'),
    bytes: zip.length,
    helpers: Object.keys(helpers),
    testedOnRemoteSsh: false,
  };
  await writeFile(`${output}.json`, `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
} finally {
  // stage is a freshly allocated child of this package's dist directory.
  assert.equal(dirname(resolve(stage)), resolve(dist));
  assert(relative(dist, stage).startsWith('.package-'));
  await rm(stage, { recursive: true, force: true });
}
