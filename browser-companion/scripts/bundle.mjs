import fs from "node:fs";
import path from "node:path";

const dist = path.resolve("dist");
for (const file of ["content.js", "background.js"]) {
  const src = path.join(dist, file.replace(".js", ".ts"));
  // tsc emits .js directly; ensure files exist
  if (!fs.existsSync(path.join(dist, file))) {
    console.warn(`missing ${file}`);
  }
}
console.log("browser-companion build complete");
