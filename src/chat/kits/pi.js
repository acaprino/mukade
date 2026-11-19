
// A Pi extension for one launch (`pi -e`): the MCP entry for this session only, the room's news
// as a message with each prompt, and, when listening, a run that goes on with what was addressed
// to this member instead of settling.
const MESSAGE = (content) => ({ customType: "mukade-chat", content, display: true });

export default function (pi) {
  if (!process.env.MUKADE_CHAT_ROOM_ID) return;
  // The credential stays in the environment; this file never holds it.
  pi.registerMcpServer("mukade", {
    url: MUKADE.endpoint,
    headers: {
      Authorization: process.env.MUKADE_CHAT_AUTHORIZATION ?? "",
      "X-Mukade-Service-Id": process.env.MUKADE_CHAT_SERVICE_ID ?? "",
    },
    exposure: "direct",
    timeout: 330,
    description: "The local mukade chat room this instance is a member of",
  });
  let listening = null;
  const input = (ctx) => {
    const session = { session_id: ctx.sessionManager.getSessionId() };
    const file = ctx.sessionManager.getSessionFile();
    if (file) session.transcript_path = file;
    return session;
  };

  // Joining: the member's name, role and rules, read with the first prompt.
  pi.on("session_start", async (_event, ctx) => {
    try {
      const answer = await mukadeHook("session-start", input(ctx));
      if (answer?.context) pi.sendMessage(MESSAGE(answer.context), { deliverAs: "nextTurn" });
    } catch {}
  });

  // What the user types ends any listening: the user goes first.
  pi.on("input", async () => {
    listening?.abort();
  });

  pi.on("before_agent_start", async (event, ctx) => {
    try {
      if (event.messageType !== "user") return;
      const answer = await mukadeHook("prompt", input(ctx));
      if (answer?.context) return { message: MESSAGE(answer.context) };
    } catch {}
  });

  // The run is about to settle: settle its batch and, when listening, go on with a message
  // addressed to this member. Escape aborts the run and the wait with it.
  pi.on("agent_before_settle", async (event, ctx) => {
    const controller = new AbortController();
    const abort = () => controller.abort();
    listening = controller;
    ctx.signal?.addEventListener("abort", abort, { once: true });
    try {
      const answer = await mukadeHook("stop", input(ctx), controller.signal);
      if (!answer?.continue || controller.signal.aborted) return;
      return {
        entries: [...event.entries, { type: "custom_message", ...MESSAGE(answer.continue) }],
        continue: true,
      };
    } catch {
      return;
    } finally {
      ctx.signal?.removeEventListener("abort", abort);
      if (listening === controller) listening = null;
    }
  });

  pi.on("session_shutdown", async () => {
    listening?.abort();
  });
}
