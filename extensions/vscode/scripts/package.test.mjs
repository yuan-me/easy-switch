import assert from 'node:assert/strict';
import test from 'node:test';
import { auditVsix, checkElf, helpers, setHelperPermissions, zipEntries } from './package-lib.mjs';

function elf(machine) {
  const data = Buffer.alloc(4096);
  data.set([0x7f, 0x45, 0x4c, 0x46, 2, 1]);
  data.writeUInt16LE(2, 16);
  data.writeUInt16LE(machine, 18);
  data.writeBigUInt64LE(64n, 32);
  data.writeUInt16LE(56, 54);
  data.writeUInt16LE(1, 56);
  data.writeUInt32LE(1, 64);
  data.writeUInt32LE(5, 68);
  data.writeBigUInt64LE(4096n, 96);
  return data;
}

function zip(files) {
  const locals = [];
  const directory = [];
  let offset = 0;
  for (const [name, data] of files) {
    const path = Buffer.from(name);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(path.length, 26);
    locals.push(local, path, data);
    const record = Buffer.alloc(46);
    record.writeUInt32LE(0x02014b50);
    record.writeUInt32LE(data.length, 20);
    record.writeUInt32LE(data.length, 24);
    record.writeUInt16LE(path.length, 28);
    record.writeUInt32LE((0o100644 << 16) >>> 0, 38);
    record.writeUInt32LE(offset, 42);
    directory.push(record, path);
    offset += local.length + path.length + data.length;
  }
  const central = Buffer.concat(directory);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50);
  end.writeUInt16LE(files.size, 8);
  end.writeUInt16LE(files.size, 10);
  end.writeUInt32LE(central.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, central, end]);
}

function fixture() {
  const expected = new Map([
    ['package.json', Buffer.from(JSON.stringify({ main: './out/extension.js', extensionKind: ['workspace'] }))],
    ['out/extension.js', Buffer.from('exports.activate = () => {};')],
    ...Object.entries(helpers).map(([name, machine]) => [name, elf(machine)]),
  ]);
  const files = new Map([
    ['[Content_Types].xml', Buffer.from('<Types/>')],
    ['extension.vsixmanifest', Buffer.from('<PackageManifest/>')],
    ...[...expected].map(([name, data]) => [`extension/${name}`, data]),
  ]);
  return { expected, files };
}

test('ELF audit refuses wrong architecture, placeholders and dynamic loader dependencies', () => {
  checkElf(elf(62), 62);
  checkElf(elf(183), 183);
  assert.throws(() => checkElf(elf(62), 183), /architecture/);
  assert.throws(() => checkElf(Buffer.from('placeholder'), 62));
  const dynamic = elf(62);
  dynamic.writeUInt32LE(3, 64);
  assert.throws(() => checkElf(dynamic, 62), /statically linked/);
});

test('VSIX contains exactly the audited files and retains Linux executable modes', () => {
  const { expected, files } = fixture();
  const data = zip(files);
  assert.throws(() => auditVsix(data, expected), /executable permissions/);
  setHelperPermissions(data);
  assert.equal(auditVsix(data, expected).fileCount, files.size);
  assert.equal(zipEntries(data).get('extension/bin/linux-arm64/easy-switch-helper').mode, 0o100755);
});

test('packaging fails closed for extra secrets, a missing helper or altered compiled content', () => {
  const { expected, files } = fixture();
  files.set('extension/auth.json', Buffer.from('{"tokens":"do-not-package"}'));
  let data = zip(files);
  setHelperPermissions(data);
  assert.throws(() => auditVsix(data, expected), /unexpected packaged file/);
  files.delete('extension/auth.json');
  files.set('extension/out/extension.js', Buffer.from('changed'));
  data = zip(files);
  setHelperPermissions(data);
  assert.throws(() => auditVsix(data, expected), /content changed/);
  files.delete('extension/bin/linux-arm64/easy-switch-helper');
  assert.throws(() => setHelperPermissions(zip(files)), /missing/);
});

test('ZIP path traversal is rejected', () => {
  assert.throws(() => zipEntries(zip(new Map([['extension/../auth.json', Buffer.from('x')]]))), /unsafe ZIP path/);
});
