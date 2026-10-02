// Verify the actual release ZIP using the same installer, without launching the app.
import { ensurePetApp } from "../extension/installer.ts";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
const root = await mkdtemp(join(tmpdir(), "omp-pet-install-smoke-"));
try {
  const options = { appOverride: "", developmentApp: join(root, "missing.app"), cacheRoot: join(root, "apps") };
  const fetchArchive = (async (input: string | URL | Request) => {
    const name = new URL(String(input)).pathname.split("/").at(-1)!;
    if (!["SHA256SUMS", "OMP-Pet-macos-arm64.zip"].includes(name)) throw new Error(`Unexpected download: ${name}`);
    return new Response(await readFile(join("dist", name)));
  }) as typeof fetch;
  const app = await ensurePetApp({ ...options, fetch: fetchArchive });
  const offline = await ensurePetApp({ ...options, fetch: (() => { throw new Error("Must reuse the cached app offline"); }) as unknown as typeof fetch });
  if (app !== offline) throw new Error("Cached installer returned a different app");
  console.log("Real release ZIP: checksum, extraction, bundle identity/version, signature and offline reuse passed; no launch.");
} finally { await rm(root, { recursive: true, force: true }); }
