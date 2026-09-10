// Loads the WebAssembly module and marshals strings across the boundary.
//
// This is the code `wasm-bindgen` used to generate. It is about fifty lines, it has no
// dependencies, and it is pinned to nothing — the ABI it speaks is described in
// crates/onyx-wasm/src/lib.rs and does not have a version to chase.
//
// WebAssembly can only pass numbers, so a string goes in as an offset and a length, and
// comes back as a pointer to a little-endian u32 length followed by that many UTF-8 bytes.

const { readFileSync } = require("node:fs");
const { join } = require("node:path");

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// ONYX_WASM_PATH overrides where the module is read from. It exists so a test can point the
// CLI at a module that is not there and exercise the engine-unavailable path, which is
// otherwise unreachable: the module ships inside the package.
function load(wasmPath = process.env.ONYX_WASM_PATH || join(__dirname, "..", "pkg", "onyx.wasm")) {
  const bytes = readFileSync(wasmPath);
  const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes), {});
  const wasm = instance.exports;

  // Growing linear memory replaces the backing ArrayBuffer, so every view has to be taken
  // fresh after any call that might allocate. Caching one is a subtle way to read garbage.
  const view = () => new Uint8Array(wasm.memory.buffer);

  // Reads a length-prefixed reply, copies it out, and returns the buffer to the module.
  function take(pointer) {
    const length = new DataView(wasm.memory.buffer).getUint32(pointer, true);
    // slice() copies: the bytes must be ours before the module reclaims them.
    const bytes = view().slice(pointer + 4, pointer + 4 + length);
    wasm.onyx_free(pointer, 4 + length);
    return decoder.decode(bytes);
  }

  function withText(entryPoint, text) {
    const input = encoder.encode(text);
    const pointer = wasm.onyx_alloc(input.length);
    // The module refuses anything over its input cap by returning null. Writing at offset
    // 0 anyway would scribble over the start of its linear memory.
    if (pointer === 0) {
      throw new Error(`document is too large for the wasm module (${input.length} bytes)`);
    }
    view().set(input, pointer);
    const reply = entryPoint(pointer, input.length);
    wasm.onyx_free(pointer, input.length);
    return take(reply);
  }

  return {
    validate: (text) => JSON.parse(withText(wasm.onyx_validate, text)),
    summary: (text) => JSON.parse(withText(wasm.onyx_summary, text)),
    // `.document` is the engine's own serialized text, as a string. Comparing it is how a
    // caller checks preservation and idempotence without a JavaScript JSON printer
    // standing in for the engine's.
    serialize: (text) => JSON.parse(withText(wasm.onyx_serialize, text)),
    specVersion: () => take(wasm.onyx_spec_version()),
    reportVersion: () => wasm.onyx_report_version(),
  };
}

module.exports = { load };
