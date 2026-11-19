// The release system shared by Shinatama, Mukade, Xiox, Muro, Ethel and artcraft-web: the same file in each, run by
// .github/workflows/release.yml at every merge of develop into main. What is project-specific is in .github/release.json:
//   app        the name of the release and of its zip
//   tagPrefix  the tag is <tagPrefix><version> (v0.1.2)
//   files      where the version is written: package.json / tauri.conf.json ("version"), Cargo.toml ([package] or
//              [workspace.package] version), Python (__version__); the first one holds the current version
//   locks      Cargo.lock files whose entries for `crates` (the project's own crates) carry the version
//   crates     the project's own crate names
//   prepare    commands run before the build on a fresh checkout (dependencies the build does not install)
//   build      the build (default: build-release.bat --no-install --ci), which fills release/
//   runner     the GitHub runner (default: windows-latest)
//   assets     zip (default: release/installer/'s installers and one zip with all of release/ in a folder named
//              after the app; no program loose beside them) or files (release/'s files)
// Usage:
//   node .github/scripts/release.mjs current                   the current version
//   node .github/scripts/release.mjs next <patch|minor|major>  the version after it
//   node .github/scripts/release.mjs bump <patch|minor|major>  writes the next version everywhere and prints it
//   node .github/scripts/release.mjs config <key>              a value of release.json (arrays: one item per line)
//   node .github/scripts/release.mjs paths                     every file `bump` writes
import { readFileSync, writeFileSync } from 'node:fs';

const DEFAULTS = { build: 'build-release.bat --no-install --ci', runner: 'windows-latest', assets: 'zip', prepare: [], locks: [], crates: [] };
const config = { ...DEFAULTS, ...JSON.parse(readFileSync('.github/release.json', 'utf8')) };
const fail = (message) => { console.error(`release: ${message}`); process.exit(1); };

const eol = (text) => (text.includes('\r\n') ? '\r\n' : '\n');
const load = (path) => { const raw = readFileSync(path, 'utf8'); return { raw, text: raw.replace(/\r\n/g, '\n'), nl: eol(raw) }; };
const save = (path, file, text) => writeFileSync(path, file.nl === '\r\n' ? text.replace(/\n/g, '\r\n') : text);
const kind = (path) => (path.endsWith('.json') ? 'json' : path.endsWith('.toml') ? 'toml' : path.endsWith('.py') ? 'python' : fail(`${path}: unknown kind`));

// Each reader returns [start, end] of the version string inside the text.
function locate(path, text) {
  if (kind(path) === 'json') {
    const version = JSON.parse(text).version;
    if (typeof version !== 'string') fail(`${path}: no top-level "version"`);
    const match = new RegExp(`("version"\\s*:\\s*")${version.replace(/\./g, '\\.')}"`).exec(text);
    if (!match) fail(`${path}: "version" not found`);
    return [match.index + match[1].length, match.index + match[1].length + version.length];
  }
  if (kind(path) === 'toml') {
    const header = /^\[(package|workspace\.package)\]\s*$/m.exec(text);
    if (!header) fail(`${path}: no [package] or [workspace.package]`);
    const rest = text.slice(header.index + header[0].length);
    const next = /^\[/m.exec(rest);
    const section = next ? rest.slice(0, next.index) : rest;
    const match = /^version\s*=\s*"([^"]+)"/m.exec(section);
    if (!match) fail(`${path}: no version in [${header[1]}]`);
    const start = header.index + header[0].length + match.index + match[0].indexOf('"') + 1;
    return [start, start + match[1].length];
  }
  const match = /^__version__\s*=\s*["']([^"']+)["']/m.exec(text);
  if (!match) fail(`${path}: no __version__`);
  const start = match.index + match[0].indexOf(match[1]);
  return [start, start + match[1].length];
}

const versionOf = (path) => { const { text } = load(path); const [a, b] = locate(path, text); return text.slice(a, b); };

function nextOf(version, level) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) fail(`${version} is not major.minor.patch`);
  const [major, minor, patch] = match.slice(1).map(Number);
  if (level === 'major') return `${major + 1}.0.0`;
  if (level === 'minor') return `${major}.${minor + 1}.0`;
  if (level === 'patch') return `${major}.${minor}.${patch + 1}`;
  return fail(`unknown step ${level}: patch, minor or major`);
}

function bump(level) {
  const current = versionOf(config.files[0]);
  const next = nextOf(current, level);
  for (const path of config.files) {
    const file = load(path);
    const [a, b] = locate(path, file.text);
    if (file.text.slice(a, b) !== current) fail(`${path} is ${file.text.slice(a, b)}, ${config.files[0]} is ${current}: align them first`);
    let text = file.text.slice(0, a) + next + file.text.slice(b);
    // package-lock.json repeats the version for the root package, under packages[""].
    if (path.endsWith('package-lock.json')) {
      const root = new RegExp(`("":\\s*\\{[^{}]*?"version"\\s*:\\s*")${current.replace(/\./g, '\\.')}"`);
      if (!root.test(text)) fail(`${path}: packages[""].version is not ${current}`);
      text = text.replace(root, `$1${next}"`);
      const check = JSON.parse(text);
      if (check.version !== next || check.packages[''].version !== next) fail(`${path}: version not written`);
    }
    save(path, file, text);
  }
  const crates = new Set(config.crates ?? []);
  for (const path of config.locks ?? []) {
    const file = load(path);
    let changed = 0;
    const blocks = file.text.split('\n[[package]]\n').map((block, i) => {
      if (i === 0) return block;
      const name = /^name = "([^"]+)"$/m.exec(block)?.[1];
      if (!crates.has(name) || /^source = /m.test(block) || !new RegExp(`^version = "${current.replace(/\./g, '\\.')}"$`, 'm').test(block)) return block;
      changed += 1;
      return block.replace(/^version = "[^"]+"$/m, `version = "${next}"`);
    });
    if (!changed) fail(`${path}: none of ${[...crates].join(', ')} is at ${current}`);
    save(path, file, blocks.join('\n[[package]]\n'));
  }
  return next;
}

const [command, arg] = process.argv.slice(2);
if (command === 'current') console.log(versionOf(config.files[0]));
else if (command === 'next') console.log(nextOf(versionOf(config.files[0]), arg));
else if (command === 'bump') console.log(bump(arg));
else if (command === 'paths') console.log([...config.files, ...(config.locks ?? [])].join('\n'));
else if (command === 'config') { const value = config[arg]; console.log(Array.isArray(value) ? value.join('\n') : value ?? ''); }
else fail('usage: current | next <step> | bump <step> | config <key> | paths');
