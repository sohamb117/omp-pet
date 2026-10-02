import { expect, test } from "bun:test";
import { createServer } from "node:net";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ExtensionAPI, RegisteredCommand, ExtensionCommandContext } from "@oh-my-pi/pi-coding-agent";
import { registerPetCommand } from "./commands.ts";

test("slash commands choose sprites and control the companion via acknowledged IPC", async () => {
  const dir = await mkdtemp(join(tmpdir(), "pet-controls-"));
  const path = join(dir, "events.sock");
  const received: any[] = [];
  const server = createServer(socket => {
    let buffer = "";
    socket.on("data", chunk => {
      buffer += chunk.toString();
      if (!buffer.includes("\n")) return;
      received.push(JSON.parse(buffer).control);
      socket.end(JSON.stringify(received.at(-1) === "reload_sprites" ? { ok: false, error: "Invalid sprite pack" } : { ok: true }) + "\n");
    });
  });
  await new Promise<void>(resolve => server.listen(path, resolve));
  const old = process.env.OMP_PET_SOCKET; process.env.OMP_PET_SOCKET = path;
  try {
    let handler: RegisteredCommand["handler"] | undefined;
    registerPetCommand({ registerCommand: (name: string, options: any) => { expect(name).toBe("pet"); handler = options.handler; } } as unknown as ExtensionAPI);
    const notices: [string, string][] = [];
    const ctx = { cwd: dir, ui: { notify: (message: string, type: string) => notices.push([message, type]) } } as unknown as ExtensionCommandContext;
    for (const arg of ["sprites", 'sprites "my sprites"', "cat", "reset", "quit", "reload"]) await handler!(arg, ctx);
    expect(received).toEqual(["choose_sprites", { load_sprites: { path: join(dir, "my sprites") } }, "use_cat", "reset_placement", "quit", "reload_sprites"]);
    expect(notices.at(-1)).toEqual(["Invalid sprite pack", "error"]);
  } finally {
    if (old === undefined) delete process.env.OMP_PET_SOCKET; else process.env.OMP_PET_SOCKET = old;
    await new Promise<void>(resolve => server.close(() => resolve())); await rm(dir, { recursive: true });
  }
});
