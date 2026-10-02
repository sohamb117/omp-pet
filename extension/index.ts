import type { ExtensionAPI, ExtensionContext } from "@oh-my-pi/pi-coding-agent";
import { basename } from "node:path";
import { bounded, type Activity, type Snapshot } from "./protocol.ts";
import { PetConnection } from "./transport.ts";

export default function petExtension(pi: ExtensionAPI): void {
  // Factories are rebound for subagents; nothing mutable lives at module scope.
  let connection: PetConnection | null = null;
  let context: ExtensionContext | null = null;
  let seq = 0;
  let activity: Activity = "idle";
  let task = "Ready for your next task";
  let running = false;
  let compacting = false;
  let failed = false;
  let lastSession: string | null = null;
  const tools = new Map<string, string>();
  const approvals = new Set<string>();
  let heartbeat: ReturnType<ExtensionContext["setInterval"]> | null = null;

  const publish = (ctx: ExtensionContext): void => {
    if (ctx.agent.kind === "sub" || !connection) return;
    context = ctx;
    const usage = ctx.getContextUsage();
    const valid = usage && Number.isFinite(usage.percent) && Number.isFinite(usage.tokens) && Number.isFinite(usage.contextWindow) && usage.contextWindow >= 1;
    const snapshot: Snapshot = {
      version: 1,
      session_id: ctx.sessionManager.getSessionId(),
      seq: ++seq,
      project: bounded(basename(ctx.cwd) || ctx.cwd, 1024),
      task: bounded(task, 1024),
      activity,
      tool: tools.size ? bounded([...tools.values()].join(" · "), 512) : null,
      context: valid ? {
        tokens: Math.max(0, Math.round(usage.tokens)),
        window: Math.round(usage.contextWindow),
        percent: Math.max(0, Math.min(100, usage.percent)),
      } : null,
    };
    // A pet failure must not become an agent failure.
    try {
      if (lastSession && lastSession !== snapshot.session_id) {
        connection.send({ ...snapshot, session_id: lastSession, seq: ++seq, activity: "disconnected", tool: null });
        snapshot.seq = ++seq;
      }
      lastSession = snapshot.session_id;
      connection.send(snapshot);
    } catch { /* IPC is optional. */ }
  };
  const recompute = (): void => {
    activity = failed ? "error" : approvals.size ? "waiting" : compacting ? "compacting" : running ? "working" : "idle";
  };

  pi.on("session_start", (_event, ctx) => {
    if (ctx.agent.kind === "sub") return;
    if (heartbeat) ctx.clearTimer(heartbeat);
    connection?.close();
    connection = new PetConnection();
    seq = 0; lastSession = null; failed = false; running = false; compacting = false; tools.clear(); approvals.clear(); recompute();
    context = ctx;
    task = pi.getSessionName() || "Ready for your next task";
    publish(ctx);
    heartbeat = ctx.setInterval(() => { if (context) publish(context); }, 5000);
  });
  pi.on("before_agent_start", (event, ctx) => {
    task = bounded(event.prompt, 300) || pi.getSessionName() || task;
    publish(ctx);
  });
  pi.on("agent_start", (_event, ctx) => { failed = false; running = true; recompute(); publish(ctx); });
  pi.on("agent_end", (_event, ctx) => {
    running = false; compacting = false; tools.clear(); approvals.clear(); recompute(); publish(ctx);
  });
  pi.on("tool_execution_start", (event, ctx) => {
    tools.set(event.toolCallId, event.intent || event.toolName); recompute(); publish(ctx);
  });
  pi.on("tool_execution_end", (event, ctx) => {
    tools.delete(event.toolCallId); recompute(); publish(ctx);
  });
  pi.on("tool_approval_requested", (event, ctx) => {
    approvals.add(event.toolCallId); recompute(); publish(ctx);
  });
  pi.on("tool_approval_resolved", (event, ctx) => {
    approvals.delete(event.toolCallId); recompute(); publish(ctx);
  });
  pi.on("auto_compaction_start", (_event, ctx) => { compacting = true; recompute(); publish(ctx); });
  pi.on("auto_compaction_end", (_event, ctx) => { compacting = false; recompute(); publish(ctx); });
  pi.on("turn_end", (_event, ctx) => publish(ctx));
  pi.on("message_end", (event, ctx) => {
    if (event.message.role === "assistant" && event.message.stopReason === "error") { failed = true; recompute(); }
    publish(ctx);
  });
  pi.on("session_switch", (_event, ctx) => {
    failed = false; running = !ctx.isIdle(); tools.clear(); approvals.clear(); compacting = false;
    task = pi.getSessionName() || "Ready for your next task"; recompute(); publish(ctx);
  });
  pi.on("session_shutdown", (_event, ctx) => {
    if (heartbeat) ctx.clearTimer(heartbeat);
    heartbeat = null;
    connection?.close(); connection = null; context = null;
  });
}
