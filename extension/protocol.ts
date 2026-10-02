export type Activity = "idle" | "working" | "waiting" | "compacting" | "error" | "disconnected";
export interface Snapshot {
  version: 1;
  session_id: string;
  seq: number;
  project: string;
  task: string;
  activity: Activity;
  tool: string | null;
  context: { tokens: number; window: number; percent: number } | null;
}

// Bound UTF-8 bytes, not just JavaScript code units, to match the Rust reader.
export function bounded(text: string, bytes: number): string {
  const cleaned = text.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim();
  let result = "";
  let length = 0;
  for (const char of cleaned) {
    const n = Buffer.byteLength(char);
    if (length + n > bytes) break;
    result += char;
    length += n;
  }
  return result;
}
