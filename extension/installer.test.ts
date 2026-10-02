import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { chmod, mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ensurePetApp, type runCommand } from "./installer.ts";

const asset = "OMP-Pet-macos-arm64.zip";
const archive = Buffer.from("test release archive");
const checksum = createHash("sha256").update(archive).digest("hex");
async function executable(app: string) {
  await mkdir(join(app, "Contents/MacOS"), { recursive: true });
  const file = join(app, "Contents/MacOS/omp-pet");
  await writeFile(file, "fixture"); await chmod(file, 0o755);
}
async function fixture(overrides: { badChecksum?: boolean; paths?: string; badSignature?: boolean; httpStatus?: number } = {}) {
  const root = await mkdtemp(join(tmpdir(), "pet-install-"));
  const urls: string[] = []; const commands: string[] = []; const notices: string[] = [];
  const fetcher = (async (input: string | URL | Request) => {
    const url = String(input); urls.push(url);
    if (overrides.httpStatus) return new Response("unavailable", { status: overrides.httpStatus });
    return new Response(url.endsWith("SHA256SUMS")
      ? `${overrides.badChecksum ? "0".repeat(64) : checksum}  ${asset}\n`
      : archive);
  }) as typeof fetch;
  const run: typeof runCommand = async (file, args) => {
    commands.push(file);
    if (file.endsWith("unzip")) return overrides.paths ?? "OMP Pet.app/\nOMP Pet.app/Contents/MacOS/omp-pet";
    if (file.endsWith("ditto")) await executable(join(args.at(-1)!, "OMP Pet.app"));
    if (file.endsWith("PlistBuddy")) return args[1]!.endsWith("CFBundleIdentifier") ? "dev.soham.omp-pet" : "0.1.1";
    if (file.endsWith("codesign") && overrides.badSignature) throw new Error("invalid signature");
    return "";
  };
  return { root, urls, commands, notices,
    options: { version: "0.1.1", platform: "darwin", arch: "arm64", appOverride: "", developmentApp: join(root, "development.app"),
      cacheRoot: join(root, "apps"), fetch: fetcher, run, onProgress: (text: string) => notices.push(text) },
    cleanup: () => rm(root, { recursive: true, force: true }) };
}

test("first install verifies, installs atomically, shares concurrent requests, then works offline", async () => {
  const f = await fixture();
  try {
    const [a, b] = await Promise.all([ensurePetApp(f.options), ensurePetApp(f.options)]);
    expect(a).toBe(join(f.root, "apps/0.1.1/OMP Pet.app")); expect(b).toBe(a);
    expect(f.urls).toEqual([
      "https://github.com/sohamb117/omp-pet/releases/download/v0.1.1/SHA256SUMS",
      `https://github.com/sohamb117/omp-pet/releases/download/v0.1.1/${asset}`,
    ]);
    expect(f.commands).toContain("/usr/bin/codesign");
    expect(f.commands).not.toContain("/usr/bin/open");
    expect(await readdir(join(f.root, "apps"))).toEqual(["0.1.1"]);
    expect(await ensurePetApp({ ...f.options, fetch: (() => { throw new Error("offline"); }) as unknown as typeof fetch })).toBe(a);
  } finally { await f.cleanup(); }
});

test("checksum mismatch never extracts and leaves no partial installation", async () => {
  const f = await fixture({ badChecksum: true });
  try {
    await expect(ensurePetApp(f.options)).rejects.toThrow("checksum mismatch");
    expect(f.commands).toEqual([]);
    expect(await readdir(join(f.root, "apps"))).toEqual([]);
  } finally { await f.cleanup(); }
});

test("unsafe archive paths are rejected before extraction", async () => {
  const f = await fixture({ paths: "OMP Pet.app/../../escape" });
  try {
    await expect(ensurePetApp(f.options)).rejects.toThrow("unexpected paths");
    expect(f.commands).toEqual(["/usr/bin/unzip"]);
    expect(await readdir(join(f.root, "apps"))).toEqual([]);
  } finally { await f.cleanup(); }
});

test("failed signature leaves an older installation untouched and allows retry", async () => {
  const f = await fixture({ badSignature: true });
  try {
    await executable(join(f.root, "apps/0.1.0/OMP Pet.app"));
    await expect(ensurePetApp(f.options)).rejects.toThrow("invalid signature");
    expect(await readdir(join(f.root, "apps"))).toEqual(["0.1.0"]);
    await expect(ensurePetApp(f.options)).rejects.toThrow("invalid signature");
    expect(f.urls.length).toBe(4);
  } finally { await f.cleanup(); }
});

test("unpublished release gives an actionable error and cleans up", async () => {
  const f = await fixture({ httpStatus: 404 });
  try {
    await expect(ensurePetApp(f.options)).rejects.toThrow("release is not available yet");
    expect(await readdir(join(f.root, "apps"))).toEqual([]);
  } finally { await f.cleanup(); }
});

test("explicit override and development builds avoid downloads; invalid override never falls back", async () => {
  const f = await fixture();
  try {
    await executable(f.options.developmentApp);
    expect(await ensurePetApp(f.options)).toBe(f.options.developmentApp);
    const override = join(f.root, "custom.app"); await executable(override);
    expect(await ensurePetApp({ ...f.options, appOverride: override })).toBe(override);
    await expect(ensurePetApp({ ...f.options, appOverride: join(f.root, "missing.app") })).rejects.toThrow("OMP_PET_APP");
    expect(f.urls).toEqual([]);
  } finally { await f.cleanup(); }
});

test("unsupported platforms and architectures fail before downloading", async () => {
  const f = await fixture();
  try {
    await expect(ensurePetApp({ ...f.options, platform: "linux" })).rejects.toThrow("requires macOS");
    await expect(ensurePetApp({ ...f.options, arch: "x64" })).rejects.toThrow("Apple Silicon");
    expect(f.urls).toEqual([]);
  } finally { await f.cleanup(); }
});
