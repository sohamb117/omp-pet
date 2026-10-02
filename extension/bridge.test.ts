import { afterEach, expect, test } from "bun:test";
import { createServer, type Server, type Socket } from "node:net";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ExtensionAPI, ExtensionContext } from "@oh-my-pi/pi-coding-agent";
import petExtension from "./index.ts";
import { bounded, type Snapshot } from "./protocol.ts";

let cleanup: (() => Promise<void>) | undefined;
afterEach(async () => { await cleanup?.(); cleanup = undefined; delete process.env.OMP_PET_SOCKET; });
async function harness(kind = "main") {
  const dir = await mkdtemp(join(tmpdir(), "omp-pet-test-"));
  process.env.OMP_PET_SOCKET = join(dir, "events.sock");
  const clients = new Set<Socket>();
  const frames: Snapshot[] = [];
  const waiters: (() => void)[] = [];
  const server = createServer(socket => {
    clients.add(socket); socket.on("close", () => clients.delete(socket));
    let buffer = "";
    socket.on("data", chunk => {
      buffer += chunk.toString();
      let i: number;
      while ((i = buffer.indexOf("\n")) >= 0) { frames.push(JSON.parse(buffer.slice(0, i))); buffer = buffer.slice(i + 1); }
      waiters.splice(0).forEach(w => w());
    });
  });
  await new Promise<void>(resolve => server.listen(process.env.OMP_PET_SOCKET!, resolve));
  const handlers = new Map<string, (event: any, ctx: ExtensionContext) => void>();
  petExtension({ on: (name: string, fn: any) => handlers.set(name, fn), getSessionName: () => "Test session" } as unknown as ExtensionAPI);
  let id = "first";
  let usage: any = { tokens: 420, contextWindow: 1000, percent: 42 };
  const ctx = { agent: { kind }, cwd: "/tmp/example", sessionManager: { getSessionId: () => id },
    getContextUsage: () => usage, isIdle: () => true,
    setInterval: () => 1, clearTimer: () => {} } as unknown as ExtensionContext;
  const emit = (name: string, event: any = {}) => handlers.get(name)!(event, ctx);
  cleanup = async () => {
    emit("session_shutdown"); clients.forEach(c => c.destroy());
    await new Promise<void>(resolve => server.close(() => resolve())); await rm(dir, { recursive: true });
  };
  async function next(name: string, event: any = {}) {
    const before = frames.length;
    emit(name, event);
    while (frames.length <= before) await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("No IPC snapshot")), 1000);
      waiters.push(() => { clearTimeout(timeout); resolve(); });
    });
    return frames.at(-1)!;
  }
  return { next, emit, frames, session: (value: string) => id = value, usage: (value: any) => usage = value };
}

test("bridge preserves overlapping approvals, tools, error state and compaction context", async () => {
  const h = await harness();
  expect((await h.next("session_start")).context?.percent).toBe(42);
  expect((await h.next("agent_start")).activity).toBe("working");
  await h.next("tool_execution_start", { toolCallId: "a", toolName: "bash", intent: "Build app" });
  await h.next("tool_execution_start", { toolCallId: "b", toolName: "read" });
  await h.next("tool_approval_requested", { toolCallId: "a" });
  await h.next("tool_approval_requested", { toolCallId: "b" });
  expect((await h.next("tool_approval_resolved", { toolCallId: "a" })).activity).toBe("waiting");
  expect((await h.next("tool_approval_resolved", { toolCallId: "b" })).activity).toBe("working");
  expect((await h.next("tool_execution_end", { toolCallId: "a" })).tool).toBe("read");
  expect((await h.next("auto_compaction_start")).activity).toBe("compacting");
  h.usage(undefined);
  expect((await h.next("auto_compaction_end")).context).toBeNull();
  await h.next("message_end", { message: { role: "assistant", stopReason: "error" } });
  expect((await h.next("agent_end")).activity).toBe("error");
  expect((await h.next("agent_start")).activity).toBe("working");
  expect((await h.next("agent_end")).activity).toBe("idle");
});
test("switching sessions retires the previous activity", async () => {
  const h = await harness(); await h.next("session_start"); await h.next("agent_start");
  h.session("second"); await h.next("session_switch");
  expect(h.frames.at(-2)?.session_id).toBe("first");
  expect(h.frames.at(-2)?.activity).toBe("disconnected");
  expect(h.frames.at(-1)?.session_id).toBe("second");
});
test("subagents never open a companion connection", async () => {
  const h = await harness("sub"); h.emit("session_start"); h.emit("agent_start"); h.emit("agent_end");
  expect(h.frames).toHaveLength(0);
});
test("bounds text by UTF-8 bytes and strips control characters", () => {
  expect(bounded("🐾🐾🐾\nhello", 8)).toBe("🐾🐾");
  expect(bounded("a\u0000b", 10)).toBe("a b");
});
