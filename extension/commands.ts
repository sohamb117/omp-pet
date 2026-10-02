import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import { execFile } from "node:child_process";
import { createConnection } from "node:net";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { socketPath } from "./transport.ts";
import { ensurePetApp } from "./installer.ts";

type Control = string | { load_sprites: { path: string } } | { resize: { size: number } };
export function requestControl(control: Control, path = socketPath()): Promise<Record<string, any>> {
  return new Promise((resolve, reject) => {
    const socket = createConnection({ path });
    let buffer = "";
    let settled = false;
    const finish = (error?: Error, response?: Record<string, any>) => {
      if (settled) return;
      settled = true; socket.destroy();
      if (error) reject(error); else resolve(response!);
    };
    socket.setTimeout(35_000, () => finish(new Error("Pet command timed out")));
    socket.on("error", error => finish(error));
    socket.on("end", () => finish(new Error("Pet closed without acknowledging the command")));
    socket.on("connect", () => socket.write(JSON.stringify({ control }) + "\n"));
    socket.on("data", chunk => {
      buffer += chunk.toString();
      if (Buffer.byteLength(buffer) > 65536) { finish(new Error("Pet response exceeds size limit")); return; }
      const end = buffer.indexOf("\n");
      if (end < 0) return;
      try {
        const response = JSON.parse(buffer.slice(0, end));
        if (!response.ok) finish(new Error(response.error || "Pet command failed"));
        else finish(undefined, response);
      } catch (error) { finish(error instanceof Error ? error : new Error(String(error))); }
    });
  });
}
export async function launchPet(onProgress?: (message: string) => void): Promise<void> {
  const app = await ensurePetApp({ onProgress });
  await new Promise<void>((resolve, reject) => execFile("/usr/bin/open", ["-g", app], error => error ? reject(error) : resolve()));
}
const unavailable = (error: unknown): boolean => ["ENOENT", "ECONNREFUSED"].includes((error as NodeJS.ErrnoException)?.code ?? "");
export async function showPet(
  request = requestControl,
  launch = launchPet,
  wait = () => new Promise<void>(resolve => setTimeout(resolve, 100)),
): Promise<Record<string, any>> {
  try { return await request("show_pet"); } catch (error) { if (!unavailable(error)) throw error; }
  await launch();
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    try { return await request("show_pet"); } catch (error) { if (!unavailable(error)) throw error; }
    await wait();
  }
  throw new Error("OMP Pet.app launched but its local socket did not become ready");
}
const HELP = "/pet sprites [folder] · reload · cat · reset · quit · show · tuck · size 160 · install · status";
export function registerPetCommand(pi: ExtensionAPI): void {
  pi.registerCommand("pet", {
    description: "Configure and control your native macOS desktop pet",
    getArgumentCompletions: prefix => ["sprites", "reload", "cat", "reset", "quit", "show", "tuck", "size", "install", "status"]
      .filter(value => value.startsWith(prefix)).map(value => ({ value, label: value })),
    handler: async (args, ctx) => {
      const match = args.trim().match(/^(\S+)(?:\s+([\s\S]+))?$/);
      const command = match?.[1]?.toLowerCase();
      if (command === "install") {
        try {
          const app = await ensurePetApp({ onProgress: message => ctx.ui.notify(message, "info") });
          ctx.ui.notify(`Pet installed: ${app}`, "info");
        } catch (error) { ctx.ui.notify(error instanceof Error ? error.message : String(error), "error"); }
        return;
      }
      let control: Control;
      if (command === "size") {
        const size=Number(match?.[2]);
        if (!Number.isFinite(size) || size<64 || size>256) { ctx.ui.notify("Usage: /pet size 160 (64–256)","error"); return; }
        control={resize:{size}};
      } else if (command === "sprites") {
        let path = match?.[2]?.trim();
        if (path && ((path.startsWith('"') && path.endsWith('"')) || (path.startsWith("'") && path.endsWith("'")))) path = path.slice(1, -1);
        if (path?.startsWith("~/")) path = resolve(homedir(), path.slice(2));
        control = path ? { load_sprites: { path: resolve(ctx.cwd, path) } } : "choose_sprites";
      } else {
        const controls: Record<string, string> = { reload: "reload_sprites", cat: "use_cat", reset: "reset_placement", quit: "quit", show: "show_pet", tuck: "tuck", status: "status" };
        if (!command || !controls[command]) { ctx.ui.notify(HELP, "info"); return; }
        control = controls[command]!;
      }
      try {
        const result = command === "show" ? await showPet(requestControl, () => launchPet(message => ctx.ui.notify(message, "info"))) : await requestControl(control);
        ctx.ui.notify(command === "status" ? JSON.stringify(result, null, 2) : command === "sprites" && result.picker ? "Choose a sprite folder in the macOS dialog" : `Pet: ${command}`, "info");
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        ctx.ui.notify(/ENOENT|ECONNREFUSED/.test(message) ? "Pet is not running. Run /pet show first." : message, "error");
      }
    },
  });
}
