#!/usr/bin/env node
// Live tests: mukade against real installs of the six harnesses, each with a store of its own.
//
//   node tests/live/run.mjs [options]          (see --help, or docs/live-tests.md)
//
// It installs the harness versions pinned in tests/live/harnesses.json into `.live/` at the root of
// the repository (gitignored), fetches the mukade that the latest green Verify run of `develop`
// built, seeds every source store from fixtures/, hops every source into every destination, reads
// each result back with mukade, and has each destination harness open what was written: with a
// command of its own that needs no model where one exists, otherwise by a resume against the local
// mock model (mock-model.mjs). It prints a matrix of pass, fail and not checked per pair, and saves
// the same as `.live/reports/<stamp>.json` and `.md`.
//
// Nothing outside `.live/` is written, apart from the chat scenarios' data folder in the system's
// temporary folder (chat refuses one inside a checkout), removed when they end. Every process runs
// with HOME, the store variables and the XDG folders inside the run folder, without API keys. No
// model is called unless --models is given.
//
// Node only: no package beyond Node itself. npm installs the npm harnesses; Hermes needs git and uv.
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { startMock } from './mock-model.mjs';

// node:sqlite (OpenCode's project id) warns that it is experimental on every Node before 24.
const emitWarning = process.emitWarning;
process.emitWarning = (warning, ...rest) => (String(warning).includes('SQLite') ? undefined : emitWarning.call(process, warning, ...rest));

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, '..', '..');
const LIVE = path.join(REPO, '.live');
const IS_WIN = process.platform === 'win32';
const TABLE = JSON.parse(fs.readFileSync(path.join(HERE, 'harnesses.json'), 'utf8'));
const CATALOG = JSON.parse(fs.readFileSync(path.join(REPO, 'catalog', 'harnesses.json'), 'utf8'));
const HARNESSES = TABLE.harnesses;
const BY_ID = Object.fromEntries(HARNESSES.map((h) => [h.id, h]));
const PROMPT = 'Reply with the single word OK.';
const DEFAULT_REPO = 'acaprino/mukade';

// The fixture each source store is seeded from, and the id it was recorded under (as in
// tests/support/mod.rs).
const FIXTURES = {
  claude_code: { file: 'claude_code/source/c45db7bc-542c-4201-89de-b590c6fdb797.jsonl', id: 'c45db7bc-542c-4201-89de-b590c6fdb797' },
  codex: { file: 'codex/source/rollout-2026-10-27T16-43-08-01a17e07-93f7-7a73-85c7-9f0205836600.jsonl', id: '01a17e07-93f7-7a73-85c7-9f0205836600' },
  opencode: { file: 'opencode/source/session-export.json', id: 'ses_e82023514ffe02y4T8OoUu99VZ' },
  pi: { file: 'pi/source/2026-10-27T16-00-39-689Z_01a17e17-9f07-743d-8072-8b3b13d65ab5.jsonl', id: '01a17e17-9f07-743d-8072-8b3b13d65ab5' },
  hermes: { file: 'hermes/source/export.jsonl', id: '20261112_120000_a1b2c3' },
  deepseek_harness: { file: 'deepseek_harness/source/session.v4.jsonl.zstd', header: 'deepseek_harness/source/session.v4.jsonl', id: 'session-0e5d1a2b-0000-4000-8000-000000000001' },
};

const USAGE = `node tests/live/run.mjs [options]

  Installs the pinned harnesses into .live/, fetches the mukade CI built, hops every source into
  every destination in stores of their own, and prints a pass/fail matrix. No model is called.

  --mukade <path>      test this mukade binary instead of the one CI built
  --release <tag>      test the mukade of a GitHub Release (Windows: the portable zip); "latest" too
  --run-id <id>        the Verify run whose mukade artifact to take (default: the latest green one
                       of develop)
  --repo <owner/name>  where runs and releases are looked up (default ${DEFAULT_REPO})
  --latest             install the newest version of each harness instead of the pinned one, and
                       report both
  --models             also resume with real models, with the keys set in this shell
                       (tests/live/harnesses.json, real_models); a check whose key is not set is
                       reported as not checked
  --from <ids>         only these sources (comma-separated harness ids)
  --to <ids>           only these destinations
  --no-chat            leave out the chat scenarios
  --install-only       install the harnesses and fetch mukade, run nothing
  --clean              stop what the runner left running and delete .live/ entirely
  --help               this text

  harness ids: ${HARNESSES.map((h) => h.id).join(', ')}`;

// ------------------------------------------------------------------------------------- processes

function winQuote(arg) {
  if (arg === '') return '""';
  return /[\s"&|<>^()%!,;=]/.test(arg) ? `"${arg.replace(/"/g, '""')}"` : arg;
}

/** Stop a process and everything it started. */
function killTree(pid) {
  if (!pid) return;
  if (IS_WIN) {
    spawnSync('taskkill', ['/T', '/F', '/PID', String(pid)], { stdio: 'ignore', windowsHide: true });
  } else {
    try { process.kill(-pid, 'SIGKILL'); } catch { try { process.kill(pid, 'SIGKILL'); } catch {} }
  }
}

/**
 * Run a command with stdin closed (or fed `input`), its output captured, and a deadline that stops
 * its whole process tree. Never throws: a command that cannot start answers code -1.
 */
function run(cmd, args, { env = process.env, cwd = REPO, timeout = 120_000, input = null } = {}) {
  return new Promise((resolve) => {
    let file = cmd;
    let argv = args;
    const opts = { env, cwd, stdio: [input == null ? 'ignore' : 'pipe', 'pipe', 'pipe'], windowsHide: true };
    if (IS_WIN && /\.(cmd|bat)$/i.test(cmd)) {
      // A .cmd launcher runs only through cmd.exe.
      file = process.env.ComSpec || 'cmd.exe';
      argv = ['/d', '/s', '/c', `"${[cmd, ...args].map(winQuote).join(' ')}"`];
      opts.windowsVerbatimArguments = true;
    }
    if (!IS_WIN) opts.detached = true; // its own process group, so the deadline stops the tree
    const started = Date.now();
    let child;
    try {
      child = spawn(file, argv, opts);
    } catch (error) {
      resolve({ code: -1, stdout: '', stderr: String(error), timedOut: false, ms: 0 });
      return;
    }
    const out = [];
    const err = [];
    child.stdout.on('data', (c) => out.push(c));
    child.stderr.on('data', (c) => err.push(c));
    if (input != null) child.stdin.end(input);
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; killTree(child.pid); }, timeout);
    let spawnError = null;
    child.on('error', (error) => { spawnError = error; });
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      resolve({
        code: spawnError ? -1 : (code ?? (signal ? 128 : -1)),
        stdout: Buffer.concat(out).toString('utf8'),
        stderr: Buffer.concat(err).toString('utf8') + (spawnError ? String(spawnError) : ''),
        timedOut,
        ms: Date.now() - started,
      });
    });
  });
}

/** The processes whose command line or executable lies under `root`, this runner and its parents aside. */
function processesUnder(root) {
  const mine = new Set([process.pid, process.ppid]);
  const found = [];
  if (process.platform === 'linux') {
    for (const entry of fs.readdirSync('/proc')) {
      if (!/^\d+$/.test(entry)) continue;
      const pid = Number(entry);
      if (mine.has(pid)) continue;
      let cmdline = '';
      let exe = '';
      try { cmdline = fs.readFileSync(`/proc/${pid}/cmdline`, 'utf8').replace(/\0/g, ' '); } catch {}
      try { exe = fs.readlinkSync(`/proc/${pid}/exe`); } catch {}
      if (cmdline.includes(root) || exe.startsWith(root)) found.push({ pid, cmd: cmdline.slice(0, 160) });
    }
  } else if (IS_WIN) {
    const script = `Get-CimInstance Win32_Process | Where-Object { ($_.CommandLine -and $_.CommandLine.Contains('${root.replace(/'/g, "''")}')) -or ($_.ExecutablePath -and $_.ExecutablePath.StartsWith('${root.replace(/'/g, "''")}')) } | ForEach-Object { "$($_.ProcessId)\`t$($_.CommandLine)" }`;
    const res = spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', windowsHide: true });
    for (const line of (res.stdout || '').split(/\r?\n/)) {
      const [pid, cmd = ''] = line.split('\t');
      if (/^\d+$/.test(pid) && !mine.has(Number(pid))) found.push({ pid: Number(pid), cmd: cmd.slice(0, 160) });
    }
  } else {
    const res = spawnSync('ps', ['-axo', 'pid=,command='], { encoding: 'utf8' });
    for (const line of (res.stdout || '').split('\n')) {
      const m = line.trim().match(/^(\d+)\s+(.*)$/);
      if (m && m[2].includes(root) && !mine.has(Number(m[1]))) found.push({ pid: Number(m[1]), cmd: m[2].slice(0, 160) });
    }
  }
  return found;
}

/** Stop every process left running under `root`; returns what was stopped. */
function sweep(root) {
  const left = processesUnder(root);
  for (const p of left) killTree(p.pid);
  return left;
}

// ------------------------------------------------------------------------------------- helpers

const log = (...a) => console.log(...a);
const note = (s) => process.stdout.write(s);
const short = (s, n = 400) => (s ?? '').toString().replace(/\s+/g, ' ').trim().slice(0, n);
const exists = (p) => fs.existsSync(p);
const mkdir = (p) => fs.mkdirSync(p, { recursive: true });

/** The last JSON object a command printed on stdout. */
function lastJson(text) {
  const t = (text ?? '').trim();
  try { return JSON.parse(t); } catch {}
  const lines = t.split(/\r?\n/).reverse();
  for (const line of lines) { try { return JSON.parse(line); } catch {} }
  const start = t.indexOf('{');
  if (start >= 0) { try { return JSON.parse(t.slice(start)); } catch {} }
  return null;
}

/** dsh's encodeSegment and projectKey (src/writer/deepseek_harness.rs). */
function dshSafe(unit) {
  return unit < 128 && /[A-Za-z0-9._-]/.test(String.fromCharCode(unit));
}
function dshEncodeSegment(raw) {
  if (raw === '.') return '~002E';
  if (raw === '..') return '~002E~002E';
  let out = '';
  for (let i = 0; i < raw.length; i++) {
    const unit = raw.charCodeAt(i);
    out += dshSafe(unit) ? raw[i] : `~${unit.toString(16).toUpperCase().padStart(4, '0')}`;
  }
  return out;
}
function dshProjectKey(cwd) {
  let readable = '';
  let inSep = false;
  for (let i = 0; i < cwd.length; i++) {
    const unit = cwd.charCodeAt(i);
    if (unit === 0x2f || unit === 0x5c || unit === 0x3a) {
      if (!inSep) readable += '-';
      inSep = true;
    } else {
      readable += dshSafe(unit) ? cwd[i] : `~${unit.toString(16).toUpperCase().padStart(4, '0')}`;
      inSep = false;
    }
  }
  const trimmed = readable.replace(/^-+/, '');
  return `--${(trimmed || 'root').slice(0, 251)}--`;
}

/** The first text a user typed in a dumped session: what a destination must send its model. */
function firstUserText(doc) {
  for (const m of doc?.messages ?? []) {
    if (m.role !== 'user') continue;
    for (const part of Array.isArray(m.content) ? m.content : [{ type: 'text', text: m.content }]) {
      if (part?.type === 'text' && typeof part.text === 'string') {
        const text = part.text.replace(/\s+/g, ' ').trim();
        if (text.length >= 8 && !text.startsWith('<')) return text;
      }
    }
  }
  return null;
}

/** Whether `snippet` appears, JSON-escaped or not, in a logged request body. */
function bodyCarries(entries, snippet) {
  if (!snippet) return false;
  const words = snippet.split(' ').filter(Boolean).slice(0, 4);
  return entries.some((e) => {
    const text = JSON.stringify(e.body ?? {}).replace(/\\\\/g, '\\').replace(/\\n|\\r|\\t/g, ' ').replace(/\s+/g, ' ');
    return words.every((w) => text.includes(w.replace(/\\/g, '\\')));
  });
}

// ------------------------------------------------------------------------------------- installs

function binPath(h, dir) {
  if (h.install.kind === 'npm') return path.join(dir, 'node_modules', '.bin', IS_WIN ? `${h.bin}.cmd` : h.bin);
  return path.join(dir, 'venv', IS_WIN ? 'Scripts' : 'bin', IS_WIN ? `${h.bin}.exe` : h.bin);
}
function hermesPython(dir) {
  return path.join(dir, 'venv', IS_WIN ? 'Scripts' : 'bin', IS_WIN ? 'python.exe' : 'python');
}
const NPM = IS_WIN ? 'npm.cmd' : 'npm';

/** The newest released version of a harness. */
async function newestVersion(h) {
  if (h.install.kind === 'npm') {
    const res = await run(NPM, ['view', h.install.package, 'version'], { timeout: 60_000 });
    return res.code === 0 ? res.stdout.trim().split(/\s+/).pop() : null;
  }
  const res = await run('git', ['ls-remote', '--tags', '--refs', h.install.repository], { timeout: 60_000 });
  if (res.code !== 0) return null;
  const pattern = new RegExp(h.install.tag_pattern ?? '^v\\d+\\.\\d+\\.\\d+$');
  const tags = res.stdout.split('\n').map((l) => l.split('refs/tags/')[1]?.trim()).filter((t) => t && pattern.test(t));
  const key = (t) => t.slice(1).split('.').map(Number);
  tags.sort((a, b) => { const x = key(a); const y = key(b); return x[0] - y[0] || x[1] - y[1] || x[2] - y[2]; });
  return tags.length ? tags[tags.length - 1].slice(1) : null;
}

/** Install one harness at `version` into its own folder, unless it is there already. */
async function install(h, version) {
  const dir = path.join(LIVE, 'harnesses', h.id, version);
  const marker = path.join(dir, '.installed');
  const bin = binPath(h, dir);
  if (exists(marker) && exists(bin)) return { dir, bin, fresh: false };
  fs.rmSync(dir, { recursive: true, force: true });
  mkdir(dir);
  const logFile = path.join(dir, 'install.log');
  let res;
  if (h.install.kind === 'npm') {
    const args = ['install', '--prefix', dir, '--no-audit', '--no-fund', '--loglevel=error'];
    if (h.install.ignore_scripts) args.push('--ignore-scripts');
    args.push(`${h.install.package}@${version}`);
    res = await run(NPM, args, { timeout: 20 * 60_000 });
    fs.writeFileSync(logFile, res.stdout + res.stderr);
  } else if (h.install.kind === 'uv-git') {
    const tag = version === h.install.version ? h.install.tag : `v${version}`;
    res = await run('git', ['clone', '--quiet', '--depth', '1', '--branch', tag, h.install.repository, path.join(dir, 'src')], { timeout: 20 * 60_000 });
    let text = res.stdout + res.stderr;
    if (res.code === 0) {
      res = await run('uv', ['sync', '--frozen', '--no-dev', '--python', h.install.python], {
        cwd: path.join(dir, 'src'),
        env: { ...process.env, UV_PROJECT_ENVIRONMENT: path.join(dir, 'venv') },
        timeout: 30 * 60_000,
      });
      text += res.stdout + res.stderr;
      if (res.code === -1) text += '\n(uv is needed for Hermes: https://docs.astral.sh/uv/)';
    }
    fs.writeFileSync(logFile, text);
  } else {
    throw new Error(`${h.id}: unknown install kind ${h.install.kind}`);
  }
  if (res.code !== 0 || !exists(bin)) {
    throw new Error(`${h.id} ${version} did not install (exit ${res.code}); see ${logFile}: ${short(res.stderr || res.stdout, 300)}`);
  }
  fs.writeFileSync(marker, JSON.stringify({ id: h.id, version, installed: new Date().toISOString() }) + '\n');
  return { dir, bin, fresh: true };
}

// ------------------------------------------------------------------------------------- mukade

async function gh(args, timeout = 10 * 60_000) {
  const res = await run(IS_WIN ? 'gh.exe' : 'gh', args, { timeout });
  if (res.code === -1) throw new Error('gh (GitHub CLI) is needed to fetch the mukade CI built; or pass --mukade <path>');
  if (res.code !== 0) throw new Error(`gh ${args.slice(0, 2).join(' ')} failed: ${short(res.stderr || res.stdout, 300)}`);
  return res.stdout;
}

function findFile(dir, name) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) { const f = findFile(p, name); if (f) return f; } else if (entry.name.toLowerCase() === name) return p;
  }
  return null;
}

/** The mukade binary under test, and where it came from. */
async function resolveMukade(opts) {
  const exe = IS_WIN ? 'mukade.exe' : 'mukade';
  if (opts.mukade) {
    const p = path.resolve(opts.mukade);
    if (!exists(p)) throw new Error(`--mukade ${p}: no such file`);
    return { path: p, source: `given: ${p}` };
  }
  const repo = opts.repo || DEFAULT_REPO;
  if (opts.release) {
    if (!IS_WIN) throw new Error('releases carry Windows binaries only: pass --mukade <path> or use the CI artifact');
    const tagArgs = opts.release === 'latest' ? [] : [opts.release];
    const dir = path.join(LIVE, 'mukade', `release-${opts.release}`);
    let bin = exists(dir) ? findFile(dir, exe) : null;
    if (!bin) {
      fs.rmSync(dir, { recursive: true, force: true });
      mkdir(dir);
      await gh(['release', 'download', ...tagArgs, '--repo', repo, '--pattern', '*.zip', '--dir', dir]);
      for (const zip of fs.readdirSync(dir).filter((f) => f.endsWith('.zip'))) {
        const res = await run('tar', ['-xf', zip], { cwd: dir, timeout: 5 * 60_000 });
        if (res.code !== 0) throw new Error(`cannot extract ${zip}: ${short(res.stderr)}`);
      }
      bin = findFile(dir, exe);
      if (!bin) throw new Error(`the release ${opts.release} holds no ${exe}`);
    }
    return { path: bin, source: `release ${opts.release} of ${repo}` };
  }
  const osName = IS_WIN ? 'Windows' : process.platform === 'linux' ? 'Linux' : null;
  if (!osName) throw new Error(`CI builds mukade for Windows and Linux only: pass --mukade <path> on ${process.platform}`);
  let runId = opts['run-id'];
  let sha = null;
  if (!runId) {
    const runs = JSON.parse(await gh(['run', 'list', '--repo', repo, '--workflow', 'verify.yml', '--branch', 'develop', '--status', 'success', '--limit', '1', '--json', 'databaseId,headSha,displayTitle']));
    if (!runs.length) throw new Error(`no green Verify run of develop in ${repo}`);
    runId = String(runs[0].databaseId);
    sha = runs[0].headSha;
  }
  const dir = path.join(LIVE, 'mukade', `run-${runId}`);
  let bin = path.join(dir, exe);
  if (!exists(bin)) {
    fs.rmSync(dir, { recursive: true, force: true });
    mkdir(dir);
    await gh(['run', 'download', String(runId), '--repo', repo, '--name', `mukade-${osName}`, '--dir', dir]);
    bin = findFile(dir, exe);
    if (!bin) throw new Error(`the artifact mukade-${osName} of run ${runId} holds no ${exe}`);
    if (sha) fs.writeFileSync(path.join(dir, 'run.json'), JSON.stringify({ run: runId, sha }) + '\n');
  }
  if (!IS_WIN) fs.chmodSync(bin, 0o755); // a zip keeps no mode
  try { sha ??= JSON.parse(fs.readFileSync(path.join(dir, 'run.json'), 'utf8')).sha; } catch {}
  return { path: bin, source: `Verify run ${runId} of ${repo}${sha ? ` at ${sha.slice(0, 10)}` : ''}` };
}

// ------------------------------------------------------------------------------------- isolation

const STORE_VARS = HARNESSES.map((h) => h.store.env);
const EXE_VARS = Object.fromEntries(CATALOG.harnesses.map((c) => [c.id, c.binary.env]));
const KEY_VARS = ['ANTHROPIC_API_KEY', 'ANTHROPIC_AUTH_TOKEN', 'CLAUDE_CODE_OAUTH_TOKEN', 'OPENAI_API_KEY', 'CODEX_API_KEY',
  'OPENROUTER_API_KEY', 'DEEPSEEK_API_KEY', 'GEMINI_API_KEY', 'GOOGLE_API_KEY', 'XAI_API_KEY', 'MISTRAL_API_KEY', 'GROQ_API_KEY', 'NOUS_API_KEY'];
const STRIP_PREFIXES = ['CLAUDE_CODE_', 'CLAUDECODE', 'CODEX_', 'OPENCODE', 'PI_', 'HERMES_', 'DSH_', 'MUKADE_', 'XDG_', 'ANTHROPIC_', 'OPENAI_'];

/**
 * The environment every harness and mukade process of a run starts from: the shell's, without
 * any harness's store, configuration or key, with HOME and the XDG folders inside the run.
 */
function baseEnv(ctx) {
  const keep = new Set(ctx.models ? Object.values(TABLE.real_models).map((r) => r.key_env).filter(Boolean) : []);
  const env = {};
  for (const [k, v] of Object.entries(process.env)) {
    const upper = k.toUpperCase();
    if (keep.has(upper)) { env[k] = v; continue; }
    if (STORE_VARS.includes(upper) || KEY_VARS.includes(upper)) continue;
    if (STRIP_PREFIXES.some((p) => upper.startsWith(p))) continue;
    env[k] = v;
  }
  const home = ctx.home;
  Object.assign(env, {
    HOME: home,
    USERPROFILE: home,
    XDG_DATA_HOME: path.join(home, '.local', 'share'),
    XDG_CONFIG_HOME: path.join(home, '.config'),
    XDG_STATE_HOME: path.join(home, '.local', 'state'),
    XDG_CACHE_HOME: path.join(home, '.cache'),
    MUKADE_DATA_DIR: ctx.dataDir,
    // Nothing a harness does on its own when it starts: no update check, telemetry or install.
    DISABLE_AUTOUPDATER: '1',
    DISABLE_TELEMETRY: '1',
    DISABLE_ERROR_REPORTING: '1',
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1',
    PI_OFFLINE: '1',
    PI_SKIP_VERSION_CHECK: '1',
    PI_TELEMETRY: '0',
    HERMES_DISABLE_LAZY_INSTALLS: '1',
    DO_NOT_TRACK: '1',
    NO_UPDATE_NOTIFIER: '1',
  });
  if (IS_WIN) {
    env.APPDATA = path.join(home, 'AppData', 'Roaming');
    env.LOCALAPPDATA = path.join(home, 'AppData', 'Local');
  }
  for (const h of HARNESSES) env[EXE_VARS[h.id]] = ctx.bins[h.id];
  return env;
}

/** A harness's variables for its home: the adapter's sandbox variables and its store variable. */
function homeEnv(h, home) {
  const env = {};
  for (const [k, v] of Object.entries(h.env)) env[k] = path.join(home, v);
  env[h.store.env] = path.join(home, h.store.path);
  return env;
}
const storeRoot = (h, home) => path.join(home, h.store.path);

/** Make a harness's home: its store folders, and a configuration that points it at the mock (or a real provider). */
function prepareHome(ctx, h, home) {
  const store = storeRoot(h, home);
  if (h.id === 'opencode') mkdir(path.dirname(store)); else mkdir(store);
  for (const f of h.store.folders) mkdir(path.join(store, f));
  for (const v of Object.values(h.env)) mkdir(path.join(home, v));
  const real = ctx.models ? TABLE.real_models[h.id] : null;
  const mock = ctx.mock.url;
  switch (h.id) {
    case 'codex': {
      const lines = real
        ? [`model_provider = "${real.provider}"`, ...(real.model ? [`model = "${real.model}"`] : [])]
        : ['model = "mock-model"', 'model_provider = "mock"', '', '[model_providers.mock]', 'name = "mock"',
          `base_url = "${mock}/v1"`, 'wire_api = "responses"', 'env_key = "MOCK_API_KEY"'];
      fs.writeFileSync(path.join(store, 'config.toml'), lines.join('\n') + '\n');
      break;
    }
    case 'opencode': {
      const config = real
        ? { $schema: 'https://opencode.ai/config.json', model: `${real.provider}/${real.model}`, autoupdate: false, share: 'disabled' }
        : {
          $schema: 'https://opencode.ai/config.json', model: 'mock/mock-model', autoupdate: false, share: 'disabled',
          provider: { mock: { npm: '@ai-sdk/openai-compatible', name: 'mock', options: { baseURL: `${mock}/v1`, apiKey: 'mock-key' }, models: { 'mock-model': { name: 'mock-model' } } } },
        };
      mkdir(path.join(home, 'config', 'opencode'));
      fs.writeFileSync(path.join(home, 'config', 'opencode', 'opencode.json'), JSON.stringify(config, null, 2) + '\n');
      break;
    }
    case 'pi': {
      if (real) {
        fs.writeFileSync(path.join(store, 'settings.json'), JSON.stringify({ defaultProvider: real.provider, defaultModel: real.model, quietStartup: true }) + '\n');
      } else {
        fs.writeFileSync(path.join(store, 'models.json'), JSON.stringify({ providers: { mock: { baseUrl: mock, api: 'anthropic-messages', apiKey: 'mock-key', models: [{ id: 'mock-model', input: ['text', 'image'] }] } } }) + '\n');
        fs.writeFileSync(path.join(store, 'settings.json'), JSON.stringify({ defaultProvider: 'mock', defaultModel: 'mock-model', quietStartup: true }) + '\n');
      }
      break;
    }
    case 'deepseek_harness': {
      const patch = real
        ? ['- id: llm-pi-ai', '  config:', '    providers:', `      ${real.provider}:`, `        apiKeyEnv: ${real.key_env}`,
          '- id: agent-default-model', '  config:', `    provider: ${real.provider}`, `    model: ${real.model}`]
        : ['- id: llm-pi-ai', '  config:', '    providers:', '      mock:', '        apiKeyEnv: MOCK_API_KEY', '        api: anthropic-messages',
          `        baseURL: ${mock}`, '        models:', '          - id: mock-model',
          '- id: agent-default-model', '  config:', '    provider: mock', '    model: mock-model'];
      fs.writeFileSync(path.join(home, 'live.patch.yml'), patch.join('\n') + '\n');
      break;
    }
    default:
      break;
  }
}

// ------------------------------------------------------------------------------------- mukade calls

function mukade(ctx, args, { env, timeout = 300_000, cwd } = {}) {
  return run(ctx.mukade, args, { env: env ?? baseEnv(ctx), timeout, cwd: cwd ?? ctx.work });
}

/** List a store with mukade and find `id`. */
async function listed(ctx, h, home, id) {
  const env = { ...baseEnv(ctx), ...homeEnv(h, home) };
  const res = await mukade(ctx, ['list', h.id, '--root', storeRoot(h, home)], { env, timeout: 120_000 });
  if (res.code !== 0) return { ok: false, why: `list exit ${res.code}: ${short(res.stderr)}` };
  const rows = res.stdout.split(/\r?\n/).filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
  const row = rows.find((r) => r.id === id);
  if (!row) return { ok: false, why: `list has no ${id} (${rows.length} rows)` };
  if (row.unreadable) return { ok: false, why: `listed as unreadable: ${short(JSON.stringify(row.unreadable), 200)}` };
  return { ok: true, row };
}

/** Dump a session with mukade; the document, or why not. */
async function dumped(ctx, h, home, id, out) {
  const env = { ...baseEnv(ctx), ...homeEnv(h, home) };
  const res = await mukade(ctx, ['dump', h.id, id, '--root', storeRoot(h, home), '--output', out], { env, timeout: 120_000 });
  if (res.code !== 0) return { ok: false, why: `dump exit ${res.code}: ${short(res.stderr)}` };
  try {
    const doc = JSON.parse(fs.readFileSync(out, 'utf8'));
    return { ok: true, doc, messages: (doc.messages ?? []).length };
  } catch (error) {
    return { ok: false, why: `dump wrote no document: ${error.message}` };
  }
}

// ------------------------------------------------------------------------------------- sources

/** Seed one source store from its fixture; resolves to its session id. */
async function seed(ctx, h, home) {
  const fixture = FIXTURES[h.id];
  const file = path.join(REPO, 'fixtures', fixture.file);
  const store = storeRoot(h, home);
  const env = { ...baseEnv(ctx), ...homeEnv(h, home) };
  switch (h.id) {
    case 'claude_code': {
      mkdir(path.join(store, 'projects', 'fixture'));
      fs.copyFileSync(file, path.join(store, 'projects', 'fixture', path.basename(file)));
      break;
    }
    case 'codex': {
      const dir = path.join(store, 'sessions', '2026', '10', '27');
      mkdir(dir);
      fs.copyFileSync(file, path.join(dir, path.basename(file)));
      break;
    }
    case 'pi': {
      const dir = path.join(store, 'sessions', '--fixture--');
      mkdir(dir);
      fs.copyFileSync(file, path.join(dir, path.basename(file)));
      break;
    }
    case 'deepseek_harness': {
      const header = JSON.parse(fs.readFileSync(path.join(REPO, 'fixtures', fixture.header), 'utf8').split('\n')[0]);
      const dir = path.join(store, 'sessions', header.cwd ? dshProjectKey(header.cwd) : '_no-cwd', dshEncodeSegment(header.id));
      mkdir(dir);
      fs.copyFileSync(file, path.join(dir, path.basename(file)));
      break;
    }
    case 'opencode': {
      // OpenCode's own import, on a private server (never the background service).
      const res = await run(ctx.bins.opencode, ['session', 'import', file, '--standalone'], { env, cwd: ctx.work, timeout: 180_000 });
      if (res.code !== 0) throw new Error(`opencode session import exit ${res.code}: ${short(res.stderr || res.stdout)}`);
      break;
    }
    case 'hermes': {
      // Hermes creates its own store; the fixture's rows (a `sessions export` line) go into it.
      const script = [
        'import json, sqlite3, sys',
        'from persistence_assembly import open_persistence, profile_scope',
        'store = open_persistence(profile_scope(), eager=True)',
        'store.close()',
        'import os',
        "db = sqlite3.connect(os.path.join(os.environ['HERMES_HOME'], 'state.db'))",
        "cols = lambda t: {r[1] for r in db.execute(f'pragma table_info({t})')}",
        'def put(t, row):',
        '    have = cols(t)',
        '    keys = [k for k in row if k in have]',
        "    vals = [json.dumps(row[k]) if isinstance(row[k], (dict, list)) else row[k] for k in keys]",
        "    db.execute(f\"insert into {t} ({','.join(keys)}) values ({','.join('?' * len(keys))})\", vals)",
        "for line in open(sys.argv[1], encoding='utf-8'):",
        '    if not line.strip(): continue',
        '    s = json.loads(line)',
        "    messages = s.pop('messages', [])",
        "    s.pop('timings', None)",
        "    put('sessions', s)",
        '    for m in messages: put(\'messages\', m)',
        'db.commit()',
      ].join('\n');
      const res = await run(hermesPython(ctx.installs.hermes.dir), ['-c', script, file], { env, cwd: ctx.work, timeout: 180_000 });
      if (res.code !== 0) throw new Error(`seeding state.db exit ${res.code}: ${short(res.stderr || res.stdout)}`);
      break;
    }
    default:
      throw new Error(`no fixture seeding for ${h.id}`);
  }
  const found = await listed(ctx, h, home, fixture.id);
  if (!found.ok) throw new Error(`mukade does not list the seeded fixture: ${found.why}`);
  return fixture.id;
}

// ------------------------------------------------------------------------------------- checks

const PASS = 'pass';
const FAIL = 'fail';
const NC = 'not checked';
const step = (status, detail) => ({ status, detail });

/** The mock's log lines written since `offset`. */
function mockSince(ctx, offset) {
  const text = fs.existsSync(ctx.mockLog) ? fs.readFileSync(ctx.mockLog, 'utf8').slice(offset) : '';
  return text.split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
}
const mockOffset = (ctx) => (fs.existsSync(ctx.mockLog) ? fs.readFileSync(ctx.mockLog, 'utf8').length : 0);

/** A resume of `id` in a harness that needs a model, against the mock or (--models) a real one. */
async function resumeWithModel(ctx, h, home, id, location, sentinel) {
  const real = ctx.models ? TABLE.real_models[h.id] : null;
  if (ctx.models && real?.key_env && !process.env[real.key_env]) return step(NC, `--models: ${real.key_env} is not set`);
  const env = { ...baseEnv(ctx), ...homeEnv(h, home) };
  if (!real) {
    env.MOCK_API_KEY = 'mock-key';
    if (h.id === 'claude_code') Object.assign(env, { ANTHROPIC_BASE_URL: ctx.mock.url, ANTHROPIC_API_KEY: 'mock-key' });
  } else if (h.id === 'codex') {
    env.CODEX_API_KEY = process.env[real.key_env];
  }
  let args;
  switch (h.id) {
    case 'claude_code': args = ['-p', '--resume', id, ...(real?.model ? ['--model', real.model] : []), PROMPT]; break;
    case 'codex': args = ['exec', '--skip-git-repo-check', 'resume', id, PROMPT]; break;
    case 'deepseek_harness': args = ['--profile', 'headless', '--patch', path.join(home, 'live.patch.yml'), '--session-id', id, PROMPT]; break;
    case 'pi': args = ['-p', '--session', location, PROMPT]; break;
    case 'opencode': args = ['run', '--standalone', '--session', id, PROMPT]; break;
    case 'hermes': args = ['chat', '-Q', '--resume', id, '--provider', real.provider, '-m', real.model, '-q', PROMPT]; break;
    default: return step(NC, 'no resume known');
  }
  const offset = mockOffset(ctx);
  const res = await run(ctx.bins[h.id], args, { env, cwd: ctx.work, timeout: 300_000 });
  const said = short(res.stdout || res.stderr, 160);
  if (res.timedOut) return step(FAIL, `resume timed out after ${Math.round(res.ms / 1000)} s`);
  if (res.code !== 0) return step(FAIL, `resume exit ${res.code}: ${short(res.stderr || res.stdout, 300)}`);
  if (real) return res.stdout.trim() ? step(PASS, `real model answered: ${said}`) : step(FAIL, 'real model answered nothing');
  if (!res.stdout.includes('MOCK-REPLY')) return step(FAIL, `the mock's reply is not in the output: ${said}`);
  const requests = mockSince(ctx, offset);
  if (!bodyCarries(requests, sentinel)) return step(FAIL, `the request sent to the model does not carry the session's first prompt (${requests.length} requests)`);
  return step(PASS, `resumed against the mock: ${said.match(/MOCK-REPLY \d+ messages/)?.[0] ?? said}, carrying the session's first prompt`);
}

/** The destination harness opens `id` with a command of its own. */
async function nativeOpen(ctx, h, home, id, location, sentinel, out) {
  const env = { ...baseEnv(ctx), ...homeEnv(h, home) };
  switch (h.id) {
    case 'opencode': {
      const list = await run(ctx.bins.opencode, ['session', 'list', '--standalone', '--format', 'json'], { env, cwd: ctx.work, timeout: 120_000 });
      if (list.code !== 0) return step(FAIL, `opencode session list exit ${list.code}: ${short(list.stderr)}`);
      if (!list.stdout.includes(id)) return step(FAIL, 'opencode session list does not name the session in its project');
      const exp = await run(ctx.bins.opencode, ['session', 'export', id, '--standalone'], { env, cwd: ctx.work, timeout: 120_000 });
      const doc = lastJson(exp.stdout);
      if (exp.code !== 0 || !doc) return step(FAIL, `opencode session export exit ${exp.code}: ${short(exp.stderr)}`);
      if (doc.info?.id !== id) return step(FAIL, `opencode session export answered ${doc.info?.id}`);
      return step(PASS, `opencode lists it and exports ${(doc.messages ?? []).length} messages`);
    }
    case 'pi': {
      const html = path.join(out, 'pi-export.html');
      const res = await run(ctx.bins.pi, ['--export', location, html], { env, cwd: ctx.work, timeout: 120_000 });
      if (res.code !== 0 || !exists(html)) return step(FAIL, `pi --export exit ${res.code}: ${short(res.stderr || res.stdout)}`);
      const m = fs.readFileSync(html, 'utf8').match(/id="session-data"[^>]*>([^<]+)</);
      if (!m) return step(FAIL, 'pi --export wrote no session data');
      const data = JSON.parse(Buffer.from(m[1].trim(), 'base64').toString('utf8'));
      if (data.header?.id !== id) return step(FAIL, `pi --export read session ${data.header?.id}`);
      const messages = (data.entries ?? []).filter((e) => e.type === 'message').length;
      return messages ? step(PASS, `pi --export reads it: ${messages} messages`) : step(FAIL, 'pi --export reads no message');
    }
    case 'hermes': {
      const res = await run(ctx.bins.hermes, ['sessions', 'export', '-', '--session-id', id, '--format', 'jsonl'], { env, cwd: ctx.work, timeout: 120_000 });
      const doc = lastJson(res.stdout.trim().split(/\r?\n/)[0]);
      if (res.code !== 0 || !doc) return step(FAIL, `hermes sessions export exit ${res.code}: ${short(res.stderr || res.stdout)}`);
      if (doc.id !== id) return step(FAIL, `hermes sessions export answered ${doc.id}`);
      const messages = (doc.messages ?? []).length;
      return messages ? step(PASS, `hermes sessions export: ${messages} messages`) : step(FAIL, 'hermes exports no message');
    }
    default:
      // Claude Code, Codex and dsh have no command that opens a session without a model.
      return resumeWithModel(ctx, h, home, id, location, sentinel);
  }
}

/** The project id OpenCode gives the work folder, read from its own store once it has seen it. */
async function opencodeProject(ctx, home) {
  const h = BY_ID.opencode;
  const env = { ...baseEnv(ctx), ...homeEnv(h, home) };
  const res = await run(ctx.bins.opencode, ['session', 'list', '--standalone'], { env, cwd: ctx.work, timeout: 120_000 });
  if (res.code !== 0) throw new Error(`opencode session list exit ${res.code}: ${short(res.stderr)}`);
  let sqlite;
  try { sqlite = await import('node:sqlite'); } catch { throw new Error('node:sqlite is needed to read OpenCode\'s project id (Node 22.5 or later)'); }
  const db = new sqlite.DatabaseSync(storeRoot(h, home), { readOnly: true });
  try {
    const rows = db.prepare('select id, worktree from project').all();
    const work = fs.realpathSync(ctx.work);
    const row = rows.find((r) => { try { return fs.realpathSync(r.worktree) === work; } catch { return false; } });
    if (!row) throw new Error(`OpenCode recorded no project for ${work} (${rows.map((r) => r.worktree).join(', ') || 'none'})`);
    return row.id;
  } finally {
    db.close();
  }
}

/** One source into one destination: the hop, the read-back and the destination's own open. */
async function pair(ctx, src, dst) {
  const name = `${src.id}->${dst.id}`;
  const out = path.join(ctx.runDir, 'out', `${src.id}--${dst.id}`);
  mkdir(out);
  const result = { source: src.id, dest: dst.id, hop: step(NC, ''), read: step(NC, ''), native: step(NC, ''), session: null };
  const srcHome = ctx.homes.src[src.id];
  const dstHome = ctx.homes.dst[dst.id];
  if (!ctx.sources[src.id]?.ok) { result.hop = step(NC, `source not seeded: ${ctx.sources[src.id]?.why}`); return result; }
  if (dst.id === 'opencode' && !ctx.opencodeProject) { result.hop = step(NC, `no OpenCode project: ${ctx.opencodeProjectWhy}`); return result; }
  const args = ['hop', src.id, ctx.sources[src.id].id, dst.id, '--root', storeRoot(src, srcHome), '--dest-root', storeRoot(dst, dstHome),
    '--cwd', ctx.work, '--manifest', path.join(out, 'manifest.json'), dst.exe_flag, ctx.bins[dst.id]];
  if (dst.id === 'opencode') args.push('--project-id', ctx.opencodeProject);
  const env = { ...baseEnv(ctx), ...homeEnv(src, srcHome), ...homeEnv(dst, dstHome) };
  const hop = await mukade(ctx, args, { env, timeout: 600_000 });
  fs.writeFileSync(path.join(out, 'hop.log'), `$ mukade ${args.join(' ')}\nexit ${hop.code} in ${hop.ms} ms\n--- stdout\n${hop.stdout}\n--- stderr\n${hop.stderr}`);
  const receipt = lastJson(hop.stdout);
  if (hop.code !== 0 || !receipt?.id) {
    result.hop = step(FAIL, `exit ${hop.code}${hop.timedOut ? ' (timed out)' : ''}: ${short(hop.stderr || hop.stdout, 300)}`);
    return result;
  }
  const counts = receipt.manifest?.counts ?? {};
  result.session = receipt.id;
  result.hop = step(PASS, `${counts.messages_in ?? '?'} → ${counts.messages_out ?? '?'} messages in ${hop.ms} ms${(receipt.manifest?.strict?.violations ?? []).length ? `, ${receipt.manifest.strict.violations.length} strict violations` : ''}`);
  result.losses = receipt.manifest?.losses ?? [];

  const found = await listed(ctx, dst, dstHome, receipt.id);
  if (!found.ok) { result.read = step(FAIL, found.why); return result; }
  const dump = await dumped(ctx, dst, dstHome, receipt.id, path.join(out, 'dump.json'));
  if (!dump.ok) { result.read = step(FAIL, dump.why); return result; }
  if (!dump.messages) { result.read = step(FAIL, 'read back with no message'); return result; }
  result.read = step(PASS, `listed and read back: ${dump.messages} messages`);

  const sentinel = firstUserText(dump.doc);
  try {
    result.native = await nativeOpen(ctx, dst, dstHome, receipt.id, receipt.location, sentinel, out);
  } catch (error) {
    result.native = step(FAIL, `the check failed: ${error.message}`);
  }
  if (ctx.models && ['pi', 'opencode', 'hermes'].includes(dst.id)) {
    result.real = await resumeWithModel(ctx, dst, dstHome, receipt.id, receipt.location, sentinel);
  }
  note(`  ${name.padEnd(36)} ${overall(result)}\n`);
  return result;
}

function overall(r) {
  const steps = [r.hop, r.read, r.native, ...(r.real ? [r.real] : [])];
  if (steps.some((s) => s.status === FAIL)) return FAIL;
  if (steps.every((s) => s.status === PASS)) return PASS;
  return NC;
}

// ------------------------------------------------------------------------------------- chat

/**
 * One launched instance per harness, in a room of its own: the service starts with a data folder
 * outside the repository, a host member writes a line, `chat launch` runs one turn against the
 * mock, and the room's members and the mock's request show whether the instance joined and was
 * given the room's news.
 */
async function chatScenarios(ctx) {
  const results = [];
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mukade-live-chat-'));
  const chatEnv = (extra = {}) => ({ ...baseEnv(ctx), MUKADE_DATA_DIR: dataDir, ...extra });
  try {
    const started = await mukade(ctx, ['chat', 'start'], { env: chatEnv(), timeout: 60_000 });
    if (started.code !== 0) {
      for (const h of HARNESSES) results.push({ harness: h.id, ...step(FAIL, `chat start exit ${started.code}: ${short(started.stderr || started.stdout)}`) });
      return results;
    }
    for (const h of HARNESSES) {
      const home = ctx.homes.chat[h.id];
      const env = chatEnv(homeEnv(h, home));
      const room = `live-${h.id.replace(/_/g, '-')}`;
      const opened = lastJson((await mukade(ctx, ['chat', 'open', '--name', room], { env, timeout: 60_000 })).stdout);
      const roomId = opened?.room_id;
      if (!roomId) { results.push({ harness: h.id, ...step(FAIL, 'chat open answered no room_id') }); continue; }
      const host = lastJson((await mukade(ctx, ['chat', 'join', '--room-id', roomId, '--harness', 'host', '--role', 'peer', '--host'], { env, timeout: 60_000 })).stdout);
      const ping = `LIVE-PING-${h.id}`;
      if (host?.member_handle) {
        await mukade(ctx, ['chat', 'send', '--room-id', roomId, '--member-handle', host.member_handle, '--text', ping], { env, timeout: 60_000 });
      }
      if (h.id === 'codex') {
        const trust = await mukade(ctx, ['chat', 'trust', 'codex'], { env, timeout: 120_000 });
        if (trust.code !== 0) { results.push({ harness: h.id, ...step(FAIL, `chat trust codex exit ${trust.code}: ${short(trust.stderr || trust.stdout)}`) }); continue; }
      }
      const turn = {
        claude_code: ['-p', PROMPT],
        codex: ['exec', '--skip-git-repo-check', PROMPT],
        opencode: ['run', PROMPT],
        pi: ['-p', PROMPT],
        hermes: ['chat', '-Q', '-q', PROMPT],
        deepseek_harness: ['--profile', 'headless', '--patch', path.join(home, 'live.patch.yml'), PROMPT],
      }[h.id];
      const launchEnv = { ...env, MOCK_API_KEY: 'mock-key', ANTHROPIC_BASE_URL: ctx.mock.url, ANTHROPIC_API_KEY: 'mock-key' };
      const offset = mockOffset(ctx);
      const launch = await mukade(ctx, ['chat', 'launch', h.id, '--room', room, '--', ...turn], { env: launchEnv, timeout: 300_000 });
      fs.writeFileSync(path.join(ctx.runDir, 'out', `chat-${h.id}.log`), `exit ${launch.code}\n--- stdout\n${launch.stdout}\n--- stderr\n${launch.stderr}`);
      if (launch.code === 2 && /adapter|not supported|no chat/i.test(launch.stderr + launch.stdout)) {
        results.push({ harness: h.id, ...step(NC, `chat launch refused: ${short(launch.stderr || launch.stdout, 200)}`) });
        continue;
      }
      const members = lastJson((await mukade(ctx, ['chat', 'members', '--room-id', roomId], { env, timeout: 60_000 })).stdout);
      const list = Array.isArray(members) ? members : members?.items ?? [];
      const joined = list.some((m) => (m.harness ?? '').includes(h.id.split('_')[0]) && !m.host);
      const requests = mockSince(ctx, offset);
      const told = requests.some((r) => JSON.stringify(r.body ?? {}).includes(ping));
      if (launch.code !== 0) results.push({ harness: h.id, ...step(FAIL, `chat launch exit ${launch.code}${launch.timedOut ? ' (timed out)' : ''}: ${short(launch.stderr || launch.stdout, 300)}`), joined, told });
      else if (!joined) results.push({ harness: h.id, ...step(FAIL, `the instance ran (exit 0) but is not a member of the room (${list.length} members)`), joined, told });
      else if (!told) results.push({ harness: h.id, ...step(FAIL, `joined, but the room's message never reached the model (${requests.length} requests)`), joined, told });
      else results.push({ harness: h.id, ...step(PASS, 'joined the room at start, and the host\'s message reached the model with the prompt'), joined, told });
      note(`  chat ${h.id.padEnd(31)} ${results[results.length - 1].status}\n`);
    }
  } finally {
    await mukade(ctx, ['chat', 'stop'], { env: chatEnv(), timeout: 60_000 });
    sweep(dataDir);
    fs.rmSync(dataDir, { recursive: true, force: true });
  }
  return results;
}

// ------------------------------------------------------------------------------------- report

function matrixText(results, sources, dests) {
  const cell = (s, d) => {
    const r = results.find((x) => x.source === s && x.dest === d);
    return r ? overall(r) : '';
  };
  const w = Math.max(16, ...dests.map((d) => d.length + 2));
  const lines = [`${'source \\ dest'.padEnd(18)}${dests.map((d) => d.padEnd(w)).join('')}`];
  for (const s of sources) lines.push(`${s.padEnd(18)}${dests.map((d) => cell(s, d).padEnd(w)).join('')}`);
  return lines.join('\n');
}

function markdown(report) {
  const { sources, dests } = report;
  const out = [];
  out.push(`# Live run ${report.started}`, '');
  out.push(`mukade: ${report.mukade.source} (\`${report.mukade.path}\`)  `);
  out.push(`platform: ${report.platform}, Node ${report.node}; models: ${report.models ? 'real (--models)' : 'none (the local mock)'}`, '');
  out.push('| harness | pinned | installed | newest |', '|---|---|---|---|');
  for (const v of report.versions) out.push(`| ${v.id} | ${v.pinned} | ${v.installed ?? '—'} | ${v.newest ?? '—'} |`);
  out.push('', '## Hops', '', `| source \\ dest | ${dests.join(' | ')} |`, `|---|${dests.map(() => '---').join('|')}|`);
  for (const s of sources) {
    out.push(`| ${s} | ${dests.map((d) => { const r = report.pairs.find((x) => x.source === s && x.dest === d); return r ? overall(r) : ''; }).join(' | ')} |`);
  }
  out.push('', '## Each pair', '');
  for (const r of report.pairs) {
    out.push(`- **${r.source} → ${r.dest}**: ${overall(r)}${r.session ? ` (\`${r.session}\`)` : ''}`);
    for (const k of ['hop', 'read', 'native', 'real']) if (r[k]) out.push(`  - ${k}: ${r[k].status}${r[k].detail ? `: ${r[k].detail}` : ''}`);
  }
  if (report.chat?.length) {
    out.push('', '## Chat', '', '| harness | result | detail |', '|---|---|---|');
    for (const c of report.chat) out.push(`| ${c.harness} | ${c.status} | ${c.detail.replace(/\|/g, '\\|')} |`);
  }
  if (report.seeds) {
    out.push('', '## Sources', '');
    for (const [id, s] of Object.entries(report.seeds)) out.push(`- ${id}: ${s.ok ? `seeded \`${s.id}\`` : `not seeded: ${s.why}`}`);
  }
  if (report.leftover?.length) out.push('', `Stopped at the end: ${report.leftover.length} processes left running (${report.leftover.map((p) => p.pid).join(', ')}).`);
  return out.join('\n') + '\n';
}

// ------------------------------------------------------------------------------------- main

function acquireLock() {
  mkdir(LIVE);
  const lock = path.join(LIVE, 'run.lock');
  try {
    const pid = Number(fs.readFileSync(lock, 'utf8'));
    if (pid && pid !== process.pid) {
      try { process.kill(pid, 0); throw new Error(`another live run (pid ${pid}) is using ${LIVE}`); } catch (error) { if (error.code !== 'ESRCH' && error.code !== 'EPERM') throw error; if (error.code === 'EPERM') throw new Error(`another live run (pid ${pid}) is using ${LIVE}`); }
    }
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  fs.writeFileSync(lock, String(process.pid));
  return () => { try { fs.rmSync(lock); } catch {} };
}

function pick(list, label) {
  if (!list) return HARNESSES.map((h) => h.id);
  const ids = list.split(',').map((s) => s.trim()).filter(Boolean);
  for (const id of ids) if (!BY_ID[id]) throw new Error(`${label}: unknown harness ${id}`);
  return ids;
}

async function main() {
  const { values: opts } = parseArgs({
    options: {
      mukade: { type: 'string' }, release: { type: 'string' }, 'run-id': { type: 'string' }, repo: { type: 'string' },
      latest: { type: 'boolean' }, models: { type: 'boolean' }, from: { type: 'string' }, to: { type: 'string' },
      'no-chat': { type: 'boolean' }, 'install-only': { type: 'boolean' }, clean: { type: 'boolean' }, help: { type: 'boolean' },
    },
    strict: true,
  });
  if (opts.help) { log(USAGE); return 0; }
  if (opts.clean) {
    const stopped = exists(LIVE) ? sweep(LIVE) : [];
    fs.rmSync(LIVE, { recursive: true, force: true });
    log(`removed ${LIVE}${stopped.length ? `; stopped ${stopped.length} processes it left running` : ''}`);
    return 0;
  }
  const sources = pick(opts.from, '--from');
  const dests = pick(opts.to, '--to');
  const release = acquireLock();
  try {
    // Installs.
    log('harnesses:');
    const versions = [];
    const installs = {};
    const bins = {};
    for (const h of HARNESSES) {
      const newest = opts.latest ? await newestVersion(h) : null;
      const version = opts.latest && newest ? newest : h.install.version;
      note(`  ${h.id.padEnd(18)} ${version.padEnd(12)} `);
      const inst = await install(h, version);
      installs[h.id] = inst;
      bins[h.id] = inst.bin;
      versions.push({ id: h.id, pinned: h.install.version, newest, version });
      note(`${inst.fresh ? 'installed' : 'ready'}\n`);
    }
    const mk = await resolveMukade(opts);
    log(`mukade: ${mk.source}\n  ${mk.path}`);
    if (opts['install-only']) return 0;

    const stamp = new Date().toISOString().replace(/[:.]/g, '-').replace('Z', '');
    const runDir = path.join(LIVE, 'runs', stamp);
    const ctx = {
      runDir, models: !!opts.models, mukade: mk.path, bins, installs,
      home: path.join(runDir, 'home'), work: path.join(runDir, 'work'), dataDir: path.join(runDir, 'mukade-data'),
      mockLog: path.join(runDir, 'mock-requests.jsonl'),
      homes: { src: {}, dst: {}, chat: {} }, sources: {},
    };
    for (const d of [ctx.home, ctx.work, ctx.dataDir, path.join(runDir, 'out')]) mkdir(d);
    fs.writeFileSync(ctx.mockLog, '');
    // The work folder is a repository of its own, so OpenCode files the sessions under a project of their own.
    const git = (args) => run('git', ['-c', 'user.name=mukade-live', '-c', 'user.email=live@localhost', '-c', 'commit.gpgsign=false', ...args], { cwd: ctx.work, env: { ...process.env, HOME: ctx.home } });
    await git(['init', '-q']);
    await git(['commit', '-q', '--allow-empty', '-m', 'mukade live run']);
    ctx.mock = await startMock(ctx.mockLog);
    log(`run: ${runDir}\nmock model: ${ctx.mock.url}`);

    for (const h of HARNESSES) {
      const res = await run(bins[h.id], ['--version'], { env: { ...baseEnv(ctx), ...homeEnv(h, path.join(runDir, 'probe', h.id)) }, timeout: 120_000 });
      versions.find((v) => v.id === h.id).installed = short((res.stdout || res.stderr).split(/\r?\n/).find((l) => /\d+\.\d+/.test(l)) ?? '', 80) || null;
    }

    const report = { started: stamp, platform: `${process.platform} ${os.release()}`, node: process.versions.node, models: ctx.models, mukade: mk, versions, sources, dests, pairs: [], chat: [], seeds: {} };
    try {
      log('sources:');
      for (const id of sources) {
        const h = BY_ID[id];
        const home = path.join(runDir, 'src', id);
        ctx.homes.src[id] = home;
        prepareHome(ctx, h, home);
        try {
          ctx.sources[id] = { ok: true, id: await seed(ctx, h, home) };
        } catch (error) {
          ctx.sources[id] = { ok: false, why: error.message };
        }
        report.seeds[id] = ctx.sources[id];
        log(`  ${id.padEnd(18)} ${ctx.sources[id].ok ? 'seeded' : `NOT SEEDED: ${ctx.sources[id].why}`}`);
      }
      for (const id of dests) {
        ctx.homes.dst[id] = path.join(runDir, 'dst', id);
        prepareHome(ctx, BY_ID[id], ctx.homes.dst[id]);
      }
      if (dests.includes('opencode')) {
        try { ctx.opencodeProject = await opencodeProject(ctx, ctx.homes.dst.opencode); } catch (error) { ctx.opencodeProjectWhy = error.message; }
      }

      log('hops:');
      for (const d of dests) {
        for (const s of sources) report.pairs.push(await pair(ctx, BY_ID[s], BY_ID[d]));
        if (d === 'hermes') await run(bins.hermes, ['gateway', 'stop'], { env: { ...baseEnv(ctx), ...homeEnv(BY_ID.hermes, ctx.homes.dst.hermes) }, timeout: 60_000 });
      }

      if (!opts['no-chat'] && !ctx.models) {
        log('chat:');
        for (const h of HARNESSES) {
          ctx.homes.chat[h.id] = path.join(runDir, 'chat', h.id);
          prepareHome(ctx, h, ctx.homes.chat[h.id]);
        }
        report.chat = await chatScenarios(ctx);
      }
    } finally {
      for (const home of [ctx.homes.src.hermes, ctx.homes.dst.hermes, ctx.homes.chat.hermes].filter(Boolean)) {
        await run(bins.hermes, ['gateway', 'stop'], { env: { ...baseEnv(ctx), ...homeEnv(BY_ID.hermes, home) }, timeout: 60_000 });
      }
      await ctx.mock.close();
      report.leftover = sweep(LIVE);
    }

    // The report.
    log('\n' + matrixText(report.pairs, sources, dests));
    const failed = report.pairs.filter((r) => overall(r) === FAIL);
    for (const r of failed) {
      log(`\n${r.source} → ${r.dest}: FAIL`);
      for (const k of ['hop', 'read', 'native', 'real']) if (r[k]?.status === FAIL) log(`  ${k}: ${r[k].detail}`);
    }
    if (report.chat.length) {
      log('\nchat:');
      for (const c of report.chat) log(`  ${c.harness.padEnd(18)} ${c.status.padEnd(12)} ${c.detail}`);
    }
    const reports = path.join(LIVE, 'reports');
    mkdir(reports);
    fs.writeFileSync(path.join(reports, `${stamp}.json`), JSON.stringify(report, null, 2) + '\n');
    fs.writeFileSync(path.join(reports, `${stamp}.md`), markdown(report));
    fs.copyFileSync(path.join(reports, `${stamp}.md`), path.join(runDir, 'report.md'));
    const counts = [PASS, FAIL, NC].map((s) => `${report.pairs.filter((r) => overall(r) === s).length} ${s}`).join(', ');
    log(`\n${report.pairs.length} pairs: ${counts}\nreport: ${path.join(reports, `${stamp}.md`)}`);
    const chatFailed = report.chat.some((c) => c.status === FAIL);
    return failed.length || chatFailed ? 1 : 0;
  } finally {
    release();
  }
}

main().then((code) => process.exit(code), (error) => { console.error(`live: ${error.message}`); process.exit(2); });
