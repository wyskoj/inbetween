// usage: node run_wasi.mjs MODULE.wasm DATA_DIR IMAGE_NAME
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';

const [wasmPath, dataDir, image] = process.argv.slice(2);
const wasi = new WASI({
  version: 'preview1',
  args: ['wasm_check', `/data/${image}`],
  preopens: { '/data': dataDir },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(wasmPath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
const t = Date.now();
const code = wasi.start(instance);
console.log(`exit code ${code}, ${Date.now() - t} ms`);
process.exit(code);
