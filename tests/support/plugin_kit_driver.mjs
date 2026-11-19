// Plays the harness around a mukade plugin kit, so the kit's JavaScript runs as the harness would
// run it, against a real `mukade chat` service. The kit's hooks call the real mukade executable.
//
// Environment: KIT (the kit file), MODE (pi | opencode), MUKADE_EXE, PEER_HANDLE (a member that
// writes to the room), MUKADE_ENDPOINT, and what `mukade chat launch` sets for the harness.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const env = process.env;

function send(text) {
  execFileSync(env.MUKADE_EXE, [
    "chat", "send", "--room-id", env.MUKADE_CHAT_ROOM_ID,
    "--member-handle", env.PEER_HANDLE, "--text", text,
  ], { stdio: ["ignore", "ignore", "pipe"] });
}

function later(ms, action) {
  return new Promise((resolve) => setTimeout(() => resolve(action()), ms));
}

async function load() {
  // Pi loads TypeScript itself; Node is given the same plain JavaScript as a module.
  const file = env.MODE === "pi" ? `${env.KIT}.mjs` : env.KIT;
  if (env.MODE === "pi") copyFileSync(env.KIT, file);
  return (await import(pathToFileURL(file).href)).default;
}

async function pi(factory) {
  const handlers = {};
  const sent = [];
  const servers = {};
  factory({
    registerMcpServer: (name, config) => { servers[name] = config; },
    on: (event, handler) => { handlers[event] = handler; },
    sendMessage: (message, options) => sent.push({ message, options }),
  });
  assert.equal(servers.mukade.url, env.MUKADE_ENDPOINT);
  assert.equal(servers.mukade.headers.Authorization, env.MUKADE_CHAT_AUTHORIZATION);
  assert.equal(servers.mukade.exposure, "direct");
  const ctx = {
    sessionManager: { getSessionId: () => "0b6f7e0e-5a8d-4c1e-9a43-3b1d1c0a7f11", getSessionFile: () => undefined },
  };

  await handlers.session_start({ reason: "startup" }, ctx);
  assert.equal(sent.length, 1);
  assert.match(sent[0].message.content, /You are pi-1, a peer/);
  assert.equal(sent[0].message.customType, "mukade-chat");
  assert.equal(sent[0].options.deliverAs, "nextTurn");

  send("hello from the peer");
  let answer = await handlers.before_agent_start({ messageType: "user", prompt: "hi" }, ctx);
  assert.match(answer.message.content, /hello from the peer/);
  assert.equal(await handlers.before_agent_start({ messageType: "custom" }, ctx), undefined);

  // Listening: a message addressed to this member makes the run go on.
  const [settled] = await Promise.all([
    handlers.agent_before_settle({ entries: [] }, ctx),
    later(1000, () => send("@pi-1 your turn")),
  ]);
  assert.equal(settled.continue, true);
  assert.equal(settled.entries.length, 1);
  assert.equal(settled.entries[0].type, "custom_message");
  assert.match(settled.entries[0].content, /your turn/);

  // What the user types ends the wait at once.
  let start = Date.now();
  const [typed] = await Promise.all([
    handlers.agent_before_settle({ entries: [] }, ctx),
    later(500, () => handlers.input({ text: "x" }, ctx)),
  ]);
  assert.equal(typed, undefined);
  assert.ok(Date.now() - start < 10000, "the wait ended with the input");

  // Escape aborts the run, and the wait with it.
  const escape = new AbortController();
  start = Date.now();
  const [aborted] = await Promise.all([
    handlers.agent_before_settle({ entries: [] }, { ...ctx, signal: escape.signal }),
    later(500, () => escape.abort()),
  ]);
  assert.equal(aborted, undefined);
  assert.ok(Date.now() - start < 10000, "the wait ended with the run");
}

async function opencode(plugin) {
  const hooks = {};
  const synthetic = [];
  let refuse = false;
  const context = {
    session: {
      hook: async (name, callback) => { hooks[name] = callback; return { dispose: async () => {} }; },
      get: async ({ sessionID }) => ({ id: sessionID, parentID: sessionID === "ses_child" ? "ses_root" : undefined }),
      synthetic: async (input) => {
        if (refuse) throw new Error("refused");
        synthetic.push(input);
        return {};
      },
    },
  };
  assert.equal(plugin.id, "mukade-chat");
  await plugin.setup(context);
  const prompt = (sessionID, text) => ({ sessionID, messageID: "msg_1", prompt: { text }, delivery: "steer" });

  let input = prompt("ses_root", "hi");
  await hooks.prompt(input);
  assert.equal(synthetic.length, 1);
  assert.match(synthetic[0].text, /You are opencode-1, a peer/);
  assert.equal(synthetic[0].resume, false);
  assert.equal(synthetic[0].delivery, "steer");
  assert.equal(input.prompt.text, "hi", "what the user typed is left alone");

  send("hello from the peer");
  await hooks.prompt(prompt("ses_root", "again"));
  assert.match(synthetic[1].text, /hello from the peer/);

  // A subagent's child session is not a member.
  await hooks.prompt(prompt("ses_child", "child"));
  await hooks["experimental.idle"]({ sessionID: "ses_child", background: [] });
  assert.equal(synthetic.length, 2);

  // Its own background work wakes it again: no waiting now.
  let start = Date.now();
  await hooks["experimental.idle"]({ sessionID: "ses_root", background: [{ kind: "shell", sessionID: "ses_root", shellID: "s", command: "sleep" }] });
  assert.ok(Date.now() - start < 5000);

  // Listening: a message addressed to this member goes on in the same execution.
  await Promise.all([
    hooks["experimental.idle"]({ sessionID: "ses_root", background: [] }),
    later(1000, () => send("@opencode-1 your turn")),
  ]);
  assert.equal(synthetic.length, 3);
  assert.match(synthetic[2].text, /your turn/);
  assert.equal(synthetic[2].resume, undefined, "it resumes the session");

  // A prompt ends the wait at once.
  start = Date.now();
  await Promise.all([
    hooks["experimental.idle"]({ sessionID: "ses_root", background: [] }),
    later(500, () => hooks.prompt(prompt("ses_root", "stop waiting"))),
  ]);
  assert.ok(Date.now() - start < 10000, "the wait ended with the prompt");
  assert.equal(synthetic.length, 3);

  // A session that refuses the synthetic message still gets the news, ahead of the prompt.
  send("one more");
  refuse = true;
  input = prompt("ses_root", "typed");
  await hooks.prompt(input);
  assert.match(input.prompt.text, /^\[mukade chat\][\s\S]*one more[\s\S]*\n\ntyped$/);
}

const kit = await load();
if (env.MODE === "pi") await pi(kit);
else await opencode(kit);
console.log("ok");
