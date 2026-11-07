import { readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import JavaScriptObfuscator from "javascript-obfuscator";

const assetsDir = resolve("frontend-dist", "assets");
const entries = await readdir(assetsDir, { withFileTypes: true });
const bundles = entries
  .filter(entry => entry.isFile() && entry.name.endsWith(".js"))
  .map(entry => resolve(assetsDir, entry.name));

if (bundles.length === 0) {
  throw new Error("Production JavaScript bundle was not found in frontend-dist/assets.");
}

const options = {
  compact: true,
  controlFlowFlattening: true,
  controlFlowFlatteningThreshold: 0.12,
  deadCodeInjection: false,
  identifierNamesGenerator: "hexadecimal",
  renameGlobals: false,
  selfDefending: false,
  splitStrings: true,
  splitStringsChunkLength: 8,
  stringArray: true,
  stringArrayEncoding: ["base64"],
  stringArrayRotate: true,
  stringArrayShuffle: true,
  stringArrayThreshold: 0.75,
  unicodeEscapeSequence: false,
  sourceMap: false,
};

for (const bundle of bundles) {
  const source = await readFile(bundle, "utf8");
  const obfuscated = JavaScriptObfuscator.obfuscate(source, options).getObfuscatedCode();
  await writeFile(bundle, obfuscated, "utf8");
  console.log(`Obfuscated ${bundle}`);
}
