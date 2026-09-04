import * as esbuild from "esbuild";
import fs from "node:fs";
import path from "node:path";

const dist = path.resolve("dist");

if (!fs.existsSync(path.join(dist, "content.js"))) {
  console.error("missing dist/content.js — run tsc first");
  process.exit(1);
}

// Chrome content_scripts cannot use ES module import/export.
// Bundle into a single classic script for injection.
await esbuild.build({
  entryPoints: [path.join(dist, "content.js")],
  bundle: true,
  outfile: path.join(dist, "content.bundle.js"),
  format: "iife",
  platform: "browser",
  target: ["chrome110"],
  logLevel: "info",
});

// Background service worker CAN use ES modules (manifest type: module).
// Still emit a bundled SW for simpler loading / fewer path issues.
await esbuild.build({
  entryPoints: [path.join(dist, "background.js")],
  bundle: true,
  outfile: path.join(dist, "background.bundle.js"),
  format: "esm",
  platform: "browser",
  target: ["chrome110"],
  logLevel: "info",
});

console.log("browser-companion build complete");
