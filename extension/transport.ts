import { createConnection, type Socket } from "node:net";
import { userInfo } from "node:os";
import type { Snapshot } from "./protocol.ts";

export function socketPath(): string {
  return process.env.OMP_PET_SOCKET ?? `/tmp/omp-pet-${userInfo().uid}/events.sock`;
}

/** Best effort: cache only the newest snapshot; never await IPC in an agent hook. */
export class PetConnection {
  private socket: Socket | null = null;
  private latest: string | null = null;
  private ready = false;
  private closed = false;

  constructor(private readonly path = socketPath()) {}

  send(snapshot: Snapshot): void {
    if (this.closed) return;
    const frame = JSON.stringify(snapshot) + "\n";
    if (Buffer.byteLength(frame) > 8192) return;
    this.latest = frame;
    if (this.socket && this.ready) {
      if (this.socket.writableLength > 8192) { this.socket.destroy(); return; }
      this.socket.write(frame);
      return;
    }
    if (this.socket) return;
    const socket = createConnection({ path: this.path });
    this.socket = socket;
    socket.unref();
    socket.on("connect", () => {
      if (this.socket !== socket || this.closed) return;
      this.ready = true;
      if (this.latest) socket.write(this.latest);
    });
    socket.on("error", () => socket.destroy());
    socket.on("close", () => {
      if (this.socket === socket) { this.socket = null; this.ready = false; }
    });
  }

  close(): void {
    this.closed = true;
    this.latest = null;
    this.socket?.destroy();
    this.socket = null;
    this.ready = false;
  }
}
