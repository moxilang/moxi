// web/moxi-loader.ts
//
// The REAL compilation layer for the Moxi Sandbox. Replace the mock
// compiler wrapper with this file (keep the mock's fixture as the
// fallback if you like — `isRealCompiler()` tells the UI which badge to
// show). No API key is involved anywhere in this file: compilation is
// local, free, and unlimited by construction. BYOK only ever affects the
// /api/generate (LLM) route.
//
// Expects the binary at /public/moxi/moxi_compiler.wasm — see
// WASM_BUILD.md for the two-command build.

export type MoxiVoxel = { x: number; y: number; z: number; color: string };
export type MoxiLayer = { name: string; dims: [number, number, number]; voxels: number };
export type MoxiErrorItem = {
  stage: "lex" | "parse" | "resolve" | "place" | "constraint" | "input" | "panic";
  message: string;
  line: number | null;
  col: number | null;
};
export type CompileResult =
  | { ok: true; total: number; bounds: [number[], number[]]; layers: MoxiLayer[]; voxels: MoxiVoxel[] }
  | { ok: false; errors: MoxiErrorItem[] };

type Exports = {
  memory: WebAssembly.Memory;
  moxi_alloc: (len: number) => number;
  moxi_dealloc: (ptr: number, len: number) => void;
  moxi_compile: (ptr: number, len: number) => number;
};

let exportsCache: Exports | null = null;
let loadAttempted = false;

async function load(): Promise<Exports | null> {
  if (exportsCache || loadAttempted) return exportsCache;
  loadAttempted = true;
  try {
    const res = await fetch("/moxi/moxi_compiler.wasm");
    if (!res.ok) return null;
    const { instance } = await WebAssembly.instantiateStreaming(res, {});
    exportsCache = instance.exports as unknown as Exports;
  } catch {
    exportsCache = null; // stay on the mock; UI shows MOCK COMPILER badge
  }
  return exportsCache;
}

/** True once the real .wasm has been fetched and instantiated. */
export function isRealCompiler(): boolean {
  return exportsCache !== null;
}

/** Warm the module at app start so the first BUILD isn't slowed. */
export function preloadCompiler(): void {
  void load();
}

export async function compileMoxi(source: string): Promise<CompileResult> {
  const wasm = await load();
  if (!wasm) return mockCompile(source);

  const src = new TextEncoder().encode(source);
  const srcPtr = wasm.moxi_alloc(src.length);
  new Uint8Array(wasm.memory.buffer, srcPtr, src.length).set(src);

  let resPtr = 0;
  try {
    resPtr = wasm.moxi_compile(srcPtr, src.length);
  } catch (e) {
    // A trap (panic) — surface it in the same structured shape.
    wasm.moxi_dealloc(srcPtr, src.length);
    return {
      ok: false,
      errors: [{ stage: "panic", message: `compiler trapped: ${e}`, line: null, col: null }],
    };
  }
  wasm.moxi_dealloc(srcPtr, src.length);

  // Result buffer: [u32 LE length][utf8 json]. Re-view memory AFTER the
  // call — compilation may have grown it, detaching old views.
  const view = new DataView(wasm.memory.buffer);
  const jsonLen = view.getUint32(resPtr, true);
  const jsonBytes = new Uint8Array(wasm.memory.buffer, resPtr + 4, jsonLen);
  const json = new TextDecoder().decode(jsonBytes);
  wasm.moxi_dealloc(resPtr, 4 + jsonLen);

  return JSON.parse(json) as CompileResult;
}

// ── Fallback (only when the .wasm is missing) ──────────────────────────────

function mockCompile(_source: string): CompileResult {
  const voxels: MoxiVoxel[] = [];
  for (let x = -2; x <= 2; x++)
    for (let z = -2; z <= 2; z++)
      for (let y = 0; y <= 2; y++)
        voxels.push({ x, y, z, color: y === 2 ? "#8b4513" : "#f4f4f0" });
  return {
    ok: true,
    total: voxels.length,
    bounds: [[-2, 0, -2], [2, 2, 2]],
    layers: [{ name: "mock/placeholder", dims: [5, 3, 5], voxels: voxels.length }],
    voxels,
  };
}
