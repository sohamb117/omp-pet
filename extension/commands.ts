import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import { createConnection } from "node:net";
import { homedir } from "node:os";
import { resolve } from "node:path";
import { socketPath } from "./transport.ts";

type Control = string | { load_sprites: { path: string } };
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
const HELP = "/pet sprites [folder] · reload · cat · reset · quit · show · tuck · status";
export function registerPetCommand(pi: ExtensionAPI): void {
  pi.registerCommand("pet", {
    description: "Configure and control your native macOS desktop pet",
    getArgumentCompletions: prefix => ["sprites", "reload", "cat", "reset", "quit", "show", "tuck", "status"]
      .filter(value => value.startsWith(prefix)).map(value => ({ value, label: value })),
    handler: async (args, ctx) => {
      const match = args.trim().match(/^(\S+)(?:\s+([\s\S]+))?$/);
      const command = match?.[1]?.toLowerCase();
      let control: Control;
      if (command === "sprites") {
        let path = match?.[2]?.trim();
        if (path && ((path.startsWith('"') && path.endsWith('"')) || (path.startsWith("'") && path.endsWith("'")))) path = path.slice(1, -1);
        if (path?.startsWith("~/")) path = resolve(homedir(), path.slice(2));
        control = path ? { load_sprites: { path: resolve(ctx.cwd, path) } } : "choose_sprites";
      } else {
        const controls: Record<string, string> = { reload: "reload_sprites", cat: "use_cat", reset: "reset_placement", quit: "quit", show: "readout", tuck: "tuck", status: "status" };
        if (!command || !controls[command]) { ctx.ui.notify(HELP, "info"); return; }
        control = controls[command]!;
      }
      try {
        const result = await requestControl(control);
        ctx.ui.notify(command === "status" ? JSON.stringify(result, null, 2) : command === "sprites" && result.picker ? "Choose a sprite folder in the macOS dialog" : `Pet: ${command}`, "info");
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        ctx.ui.notify(/ENOENT|ECONNREFUSED/.test(message) ? "Pet is not running. Open OMP Pet.app first." : message, "error");
      }
    },
  });
}
