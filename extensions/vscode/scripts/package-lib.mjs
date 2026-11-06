import assert from 'node:assert/strict';
import { inflateRawSync } from 'node:zlib';

export const helpers = {
  'bin/linux-x64/easy-switch-helper': 62,
  'bin/linux-arm64/easy-switch-helper': 183,
};

export function checkElf(data, machine) {
  assert(data.length >= 4096, 'helper is missing or too small to be an executable');
  assert.equal(data.subarray(0, 4).toString('hex'), '7f454c46', 'helper must be a Linux ELF binary');
  assert.equal(data[4], 2, 'helper must be 64-bit');
  assert.equal(data[5], 1, 'helper must be little-endian');
  assert([2, 3].includes(data.readUInt16LE(16)), 'helper must be executable');
  assert.equal(data.readUInt16LE(18), machine, 'helper architecture does not match its directory');
  const offset = Number(data.readBigUInt64LE(32));
  const entrySize = data.readUInt16LE(54);
  const count = data.readUInt16LE(56);
  assert(entrySize >= 56 && count > 0 && offset + entrySize * count <= data.length, 'invalid ELF program table');
  let executable = false;
  for (let i = 0; i < count; i++) {
    const p = offset + i * entrySize;
    const type = data.readUInt32LE(p);
    assert.notEqual(type, 3, 'helper must be statically linked; dynamic loader dependencies are not bundled');
    const fileOffset = Number(data.readBigUInt64LE(p + 8));
    const fileSize = Number(data.readBigUInt64LE(p + 32));
    assert(fileOffset + fileSize <= data.length, 'invalid ELF segment');
    if (type === 1 && (data.readUInt32LE(p + 4) & 1) && fileSize > 0) executable = true;
  }
  assert(executable, 'ELF has no executable load segment');
}

export function zipEntries(zip) {
  let end = zip.length - 22;
  while (end >= Math.max(0, zip.length - 65557) && zip.readUInt32LE(end) !== 0x06054b50) end--;
  assert(end >= Math.max(0, zip.length - 65557), 'missing ZIP directory');
  assert.equal(zip.readUInt16LE(end + 4), 0, 'multipart ZIP is unsupported');
  assert.equal(zip.readUInt16LE(end + 6), 0, 'multipart ZIP is unsupported');
  const count = zip.readUInt16LE(end + 10);
  assert.notEqual(count, 65535, 'ZIP64 is unsupported');
  let pos = zip.readUInt32LE(end + 16);
  const entries = new Map();
  for (let i = 0; i < count; i++) {
    assert.equal(zip.readUInt32LE(pos), 0x02014b50, 'invalid ZIP directory entry');
    const flags = zip.readUInt16LE(pos + 8);
    const method = zip.readUInt16LE(pos + 10);
    const compressedSize = zip.readUInt32LE(pos + 20);
    const size = zip.readUInt32LE(pos + 24);
    const nameLength = zip.readUInt16LE(pos + 28);
    const name = zip.subarray(pos + 46, pos + 46 + nameLength).toString('utf8');
    assert(!(flags & 1), 'encrypted ZIP is unsupported');
    assert(!name.startsWith('/') && !name.includes('\\') && !name.split('/').includes('..'), 'unsafe ZIP path');
    assert(!entries.has(name), 'duplicate ZIP entry');
    const local = zip.readUInt32LE(pos + 42);
    assert.equal(zip.readUInt32LE(local), 0x04034b50, 'invalid ZIP local entry');
    const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    assert(start + compressedSize <= zip.length, 'truncated ZIP file');
    const compressed = zip.subarray(start, start + compressedSize);
    assert(method === 0 || method === 8, 'unsupported ZIP compression');
    const data = method === 8 ? inflateRawSync(compressed) : compressed;
    assert.equal(data.length, size, 'truncated ZIP contents');
    entries.set(name, { data, directoryOffset: pos, mode: zip.readUInt32LE(pos + 38) >>> 16 });
    pos += 46 + nameLength + zip.readUInt16LE(pos + 30) + zip.readUInt16LE(pos + 32);
  }
  return entries;
}

export function setHelperPermissions(zip) {
  const entries = zipEntries(zip);
  for (const name of Object.keys(helpers)) {
    const entry = entries.get(`extension/${name}`);
    assert(entry, `VSIX is missing ${name}`);
    // Windows packaging cannot express Unix execute bits through fs.chmod.
    zip.writeUInt16LE((3 << 8) | 20, entry.directoryOffset + 4);
    zip.writeUInt32LE((0o100755 << 16) >>> 0, entry.directoryOffset + 38);
  }
}

export function auditVsix(zip, expected) {
  const entries = zipEntries(zip);
  const metadata = new Set(['[Content_Types].xml', 'extension.vsixmanifest']);
  for (const [name, entry] of entries) {
    if (name.endsWith('/')) continue;
    if (metadata.has(name)) continue;
    assert(name.startsWith('extension/'), `unexpected VSIX file: ${name}`);
    const relative = name.slice('extension/'.length);
    assert(expected.has(relative), `unexpected packaged file: ${relative}`);
    assert(entry.data.equals(expected.get(relative)), `packaged content changed: ${relative}`);
  }
  for (const name of expected.keys()) assert(entries.has(`extension/${name}`), `packaged file missing: ${name}`);
  for (const name of metadata) assert(entries.has(name), `VSIX metadata missing: ${name}`);
  for (const [name, machine] of Object.entries(helpers)) {
    const entry = entries.get(`extension/${name}`);
    checkElf(entry.data, machine);
    assert.equal(entry.mode & 0o777, 0o755, `${name} has lost executable permissions`);
  }
  const manifest = JSON.parse(entries.get('extension/package.json').data.toString('utf8'));
  assert.deepEqual(manifest.extensionKind, ['workspace'], 'extension must run on the remote workspace host');
  assert.equal(manifest.main, './out/extension.js');
  return { fileCount: [...entries.keys()].filter((name) => !name.endsWith('/')).length };
}
