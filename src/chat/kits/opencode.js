
// An OpenCode 2 server plugin for one launch: the room's news reach the model as a synthetic
// message with each prompt, and a listening instance that went idle goes on with what was
// addressed to it. Only top-level sessions are members: a subagent's child session is not.
const SOURCE = { source: "mukade-chat" };

export default {
  id: "mukade-chat",
  setup: async (context) => {
    if (!process.env.MUKADE_CHAT_ROOM_ID) return;
    const listening = new Map();
    const topLevel = new Map();
    const isMember = async (sessionID) => {
      if (!topLevel.has(sessionID)) {
        let top = true;
        try {
          const info = await context.session.get({ sessionID });
          top = !info?.parentID;
        } catch {}
        topLevel.set(sessionID, top);
      }
      return topLevel.get(sessionID);
    };

    // Each prompt: what the room wrote since, ahead of what the user typed.
    await context.session.hook("prompt", async (input) => {
      try {
        if (!(await isMember(input.sessionID))) return;
        // A prompt ends any listening of this session: the user goes first.
        listening.get(input.sessionID)?.abort();
        const answer = await mukadeHook("prompt", { session_id: input.sessionID });
        const text = answer?.context;
        if (!text) return;
        try {
          await context.session.synthetic({
            sessionID: input.sessionID,
            text,
            description: "mukade chat",
            metadata: SOURCE,
            delivery: input.delivery,
            resume: false,
          });
        } catch {
          input.prompt.text = `${text}\n\n${input.prompt.text}`;
        }
      } catch {}
    });

    // The session is about to go idle: settle the turn's batch and, when listening, wait for a
    // message addressed to this member; one that comes goes on in the same execution.
    await context.session.hook("experimental.idle", async (input) => {
      try {
        if (!(await isMember(input.sessionID))) return;
        // Its own background work wakes it again; the hook comes back then.
        if (input.background?.some((job) => job.sessionID === input.sessionID)) return;
        const controller = new AbortController();
        listening.set(input.sessionID, controller);
        let answer;
        try {
          answer = await mukadeHook("stop", { session_id: input.sessionID }, controller.signal);
        } finally {
          if (listening.get(input.sessionID) === controller) listening.delete(input.sessionID);
        }
        const text = answer?.continue;
        if (!text || controller.signal.aborted) return;
        await context.session.synthetic({
          sessionID: input.sessionID,
          text,
          description: "mukade chat",
          metadata: SOURCE,
        });
      } catch {}
    });

    return () => {
      for (const controller of listening.values()) controller.abort();
    };
  },
};
