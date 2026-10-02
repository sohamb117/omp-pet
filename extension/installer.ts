import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASES = "https://github.com/sohamb117/omp-pet/releases/download";
const ASSET = "OMP-Pet-macos-arm64.zip";
const APP = "OMP Pet.app";
const installs = new Map<string, Promise<string>>();

export function runCommand(file: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => execFile(file, args, { timeout: 30_000, maxBuffer: 1024 * 1024 },
    (error, stdout) => error ? reject(error) : resolve(stdout.trim())));
}

type Options = {
  version?: string;
  platform?: string;
  arch?: string;
  appOverride?: string;
  developmentApp?: string;
  cacheRoot?: string;
  fetch?: typeof globalThis.fetch;
  run?: typeof runCommand;
  onProgress?: (message: string) => void;
};

async function runnable(app: string): Promise<boolean> {
  try { await access(join(app, "Contents/MacOS/omp-pet"), constants.X_OK); return true; }
  catch (error) {
    if (["ENOENT", "ENOTDIR"].includes((error as NodeJS.ErrnoException).code ?? "")) return false;
    throw error;
  }
}

async function download(url: string, limit: number, fetcher: typeof globalThis.fetch): Promise<Buffer> {
  const response = await fetcher(url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok) {
    throw new Error(response.status === 404
      ? "The matching OMP Pet app release is not available yet. Install a published plugin version or try again after its release finishes."
      : `OMP Pet download failed (HTTP ${response.status}). Run /pet show to retry.`);
  }
  if (!response.body) throw new Error("OMP Pet download was empty");
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > limit) throw new Error("OMP Pet download exceeds the allowed size");
      chunks.push(value);
    }
  } finally { await reader.cancel(); }
  return Buffer.concat(chunks);
}

/** Downloads only on explicit /pet show or /pet install; never from agent lifecycle hooks. */
export async function ensurePetApp(options: Options = {}): Promise<string> {
  const platform = options.platform ?? process.platform;
  if (platform !== "darwin") throw new Error("OMP Pet requires macOS");
  const override = options.appOverride ?? process.env.OMP_PET_APP;
  if (override) {
    if (!await runnable(override)) throw new Error(`OMP_PET_APP does not contain a runnable OMP Pet app: ${override}`);
    return override;
  }
  const development = options.developmentApp ?? fileURLToPath(new URL("../dist/OMP Pet.app", import.meta.url));
  if (await runnable(development)) return development;
  if ((options.arch ?? process.arch) !== "arm64") {
    throw new Error("Automatic OMP Pet installation currently requires native Apple Silicon OMP. For Intel or Rosetta, build the app and set OMP_PET_APP.");
  }
  const version: string = options.version ?? JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8")).version;
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error("Invalid OMP Pet package version");
  const root = options.cacheRoot ?? join(homedir(), "Library/Application Support/OMP Pet/apps");
  const destination = join(root, version);
  const app = join(destination, APP);
  if (await runnable(app)) return app;
  const existing = installs.get(destination);
  if (existing) return existing;
  const pending = install();
  installs.set(destination, pending);
  try { return await pending; } finally { installs.delete(destination); }

  async function install(): Promise<string> {
    const fetcher = options.fetch ?? globalThis.fetch;
    const run = options.run ?? runCommand;
    options.onProgress?.(`Installing OMP Pet ${version}…`);
    await mkdir(root, { recursive: true, mode: 0o700 });
    const stage = await mkdtemp(join(root, ".install-"));
    try {
      const base = `${RELEASES}/v${version}`;
      const checksumFile = (await download(`${base}/SHA256SUMS`, 16 * 1024, fetcher)).toString("utf8");
      const entry = checksumFile.split(/\r?\n/).map(line => line.match(/^([a-fA-F0-9]{64})\s+\*?(.+)$/))
        .find(match => match?.[2] === ASSET);
      if (!entry) throw new Error("Release has no valid SHA-256 checksum for the app");
      const archive = await download(`${base}/${ASSET}`, 32 * 1024 * 1024, fetcher);
      if (createHash("sha256").update(archive).digest("hex") !== entry[1]!.toLowerCase()) {
        throw new Error("OMP Pet checksum mismatch; nothing was installed. Run /pet show to retry.");
      }
      const zip = join(stage, "app.zip");
      await writeFile(zip, archive, { mode: 0o600 });
      const entries = (await run("/usr/bin/unzip", ["-Z", "-1", zip])).split(/\r?\n/).filter(Boolean);
      if (!entries.length || entries.some(name => !name.startsWith(`${APP}/`) || name.split("/").includes("..") || name.includes("\\"))) {
        throw new Error("OMP Pet archive has unexpected paths; nothing was installed");
      }
      const extracted = join(stage, "payload");
      await mkdir(extracted);
      await run("/usr/bin/ditto", ["-x", "-k", zip, extracted]);
      const stagedApp = join(extracted, APP);
      if (!await runnable(stagedApp)) throw new Error("Downloaded app is missing its executable");
      const plist = join(stagedApp, "Contents/Info.plist");
      if (await run("/usr/libexec/PlistBuddy", ["-c", "Print :CFBundleIdentifier", plist]) !== "dev.soham.omp-pet"
        || await run("/usr/libexec/PlistBuddy", ["-c", "Print :CFBundleShortVersionString", plist]) !== version) {
        throw new Error("Downloaded app identity or version does not match this plugin");
      }
      await run("/usr/bin/codesign", ["--verify", "--deep", "--strict", stagedApp]);
      try { await rename(extracted, destination); }
      catch (error) {
        // Another OMP process may have installed the same immutable version concurrently.
        if (!["EEXIST", "ENOTEMPTY"].includes((error as NodeJS.ErrnoException).code ?? "") || !await runnable(app)) throw error;
      }
      options.onProgress?.(`OMP Pet ${version} installed.`);
      return app;
    } finally { await rm(stage, { recursive: true, force: true }); }
  }
}
