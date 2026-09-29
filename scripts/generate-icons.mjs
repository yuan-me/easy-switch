// One brand source for the React sidebar, Windows executable, and installer.
// Uses the installed Tauri CLI; does not download tools or access the network.
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('..', import.meta.url));
const cli = path.join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const result = spawnSync(process.execPath, [cli, 'icon', path.join(root, 'assets', 'app-icon.png'), '--output', path.join(root, 'src-tauri', 'icons')], {cwd: root, stdio: 'inherit', shell: false});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
