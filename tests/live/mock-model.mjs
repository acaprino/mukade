// A local stand-in for the three model APIs the harnesses speak, on 127.0.0.1 only: Anthropic
// Messages (`/v1/messages`), OpenAI Responses (`/v1/responses`) and OpenAI Chat Completions
// (`/v1/chat/completions`). Nothing leaves the machine and no key is checked.
//
// Every request is appended to the log (one JSON line: path, the body's size and the body). Every
// answer is one short streamed text, `MOCK-REPLY <n> messages`, where n is the number of messages
// or input items the request carried: a destination that sent the written history shows it in
// its own output, and the runner finds the session's texts in the logged body.
//
// Used two ways: imported by run.mjs (startMock), or run alone for a hand test:
//   node tests/live/mock-model.mjs <log-file>        prints the port, serves until killed
import http from 'node:http';
import fs from 'node:fs';

/** The messages or input items a request carries, whatever the API. */
function messageCount(path, body) {
  if (Array.isArray(body?.messages)) return body.messages.length;
  if (Array.isArray(body?.input)) return body.input.length;
  if (typeof body?.input === 'string') return 1;
  return 0;
}

function sse(res, events) {
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache', connection: 'close' });
  for (const [name, data] of events) {
    res.write((name ? `event: ${name}\n` : '') + `data: ${typeof data === 'string' ? data : JSON.stringify(data)}\n\n`);
  }
  res.end();
}

function json(res, code, payload) {
  const data = Buffer.from(JSON.stringify(payload));
  res.writeHead(code, { 'content-type': 'application/json', 'content-length': data.length, connection: 'close' });
  res.end(data);
}

function anthropic(res, body, text) {
  const message = {
    id: 'msg_mock', type: 'message', role: 'assistant', model: body.model ?? 'mock-model',
    content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 10, output_tokens: 1 },
  };
  if (body.stream === false) {
    return json(res, 200, { ...message, content: [{ type: 'text', text }], stop_reason: 'end_turn', usage: { input_tokens: 10, output_tokens: 5 } });
  }
  sse(res, [
    ['message_start', { type: 'message_start', message }],
    ['content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } }],
    ['content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text } }],
    ['content_block_stop', { type: 'content_block_stop', index: 0 }],
    ['message_delta', { type: 'message_delta', delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: { output_tokens: 5 } }],
    ['message_stop', { type: 'message_stop' }],
  ]);
}

function responses(res, body, text) {
  const item = { type: 'message', role: 'assistant', id: 'msg_mock', status: 'completed', content: [{ type: 'output_text', text, annotations: [] }] };
  const usage = { input_tokens: 10, input_tokens_details: { cached_tokens: 0 }, output_tokens: 5, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 15 };
  const response = { id: 'resp_mock', object: 'response', status: 'completed', model: body.model ?? 'mock-model', output: [item], usage };
  if (body.stream === false) return json(res, 200, response);
  sse(res, [
    ['response.created', { type: 'response.created', response: { ...response, status: 'in_progress', output: [] } }],
    ['response.output_item.added', { type: 'response.output_item.added', output_index: 0, item: { ...item, status: 'in_progress', content: [] } }],
    ['response.output_text.delta', { type: 'response.output_text.delta', output_index: 0, content_index: 0, item_id: 'msg_mock', delta: text }],
    ['response.output_item.done', { type: 'response.output_item.done', output_index: 0, item }],
    ['response.completed', { type: 'response.completed', response }],
  ]);
}

function chat(res, body, text) {
  const base = { id: 'chatcmpl_mock', object: 'chat.completion.chunk', created: Math.floor(Date.now() / 1000), model: body.model ?? 'mock-model' };
  const usage = { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 };
  if (body.stream !== true) {
    return json(res, 200, { ...base, object: 'chat.completion', choices: [{ index: 0, message: { role: 'assistant', content: text }, finish_reason: 'stop' }], usage });
  }
  sse(res, [
    [null, { ...base, choices: [{ index: 0, delta: { role: 'assistant', content: '' }, finish_reason: null }] }],
    [null, { ...base, choices: [{ index: 0, delta: { content: text }, finish_reason: null }] }],
    [null, { ...base, choices: [{ index: 0, delta: {}, finish_reason: 'stop' }], usage }],
    [null, '[DONE]'],
  ]);
}

/** Start the mock on a free port; resolves to { port, url, close() }. */
export function startMock(logFile) {
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8');
      let body = {};
      try { body = raw ? JSON.parse(raw) : {}; } catch { body = { unparsed: raw.slice(0, 2000) }; }
      const path = (req.url ?? '').split('?')[0];
      try { fs.appendFileSync(logFile, JSON.stringify({ time: new Date().toISOString(), method: req.method, path, bytes: raw.length, body }) + '\n'); } catch {}
      if (req.method === 'GET') {
        // Model listings some harnesses ask for at start.
        if (path.endsWith('/models')) return json(res, 200, { object: 'list', data: [{ id: 'mock-model', object: 'model', created: 0, owned_by: 'mock' }] });
        return json(res, 404, { error: { type: 'not_found', message: 'mock' } });
      }
      const text = `MOCK-REPLY ${messageCount(path, body)} messages`;
      if (path.endsWith('/messages')) return anthropic(res, body, text);
      if (path.endsWith('/responses')) return responses(res, body, text);
      if (path.endsWith('/chat/completions')) return chat(res, body, text);
      if (path.endsWith('/count_tokens')) return json(res, 200, { input_tokens: 10 });
      return json(res, 404, { error: { type: 'not_found', message: `mock has no ${path}` } });
    });
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({ port, url: `http://127.0.0.1:${port}`, close: () => new Promise((r) => { server.closeAllConnections?.(); server.close(() => r()); }) });
    });
  });
}

if (import.meta.url === `file://${process.argv[1]}` || process.argv[1]?.endsWith('mock-model.mjs')) {
  const log = process.argv[2];
  if (!log) { console.error('usage: node tests/live/mock-model.mjs <log-file>'); process.exit(1); }
  const mock = await startMock(log);
  console.log(mock.port);
}
