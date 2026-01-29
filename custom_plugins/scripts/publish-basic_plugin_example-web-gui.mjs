import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const webGuiDir = path.join(repoRoot, "basic_plugin_example", "web-gui");
const distDir = path.join(webGuiDir, "dist");
const embeddedPath = path.join(webGuiDir, "embedded.html");

execSync("pnpm --dir " + webGuiDir + " build", { stdio: "inherit" });

let html = readFileSync(path.join(distDir, "index.html"), "utf8");

const cssLinks = [...html.matchAll(/<link[^>]+href="([^"]+\.css)"[^>]*>/g)];
const jsScripts = [...html.matchAll(/<script[^>]+src="([^"]+\.js)"[^>]*><\\/script>/g)];

for (const match of cssLinks) {
  const href = match[1];
  const assetPath = href.startsWith("/") ? href.slice(1) : href;
  const css = readFileSync(path.resolve(distDir, assetPath), "utf8");
  html = html.replace(match[0], `<style>\n${css}\n</style>`);
}

for (const match of jsScripts) {
  const src = match[1];
  const assetPath = src.startsWith("/") ? src.slice(1) : src;
  const js = readFileSync(path.resolve(distDir, assetPath), "utf8");
  html = html.replace(match[0], `<script type=\"module\">\n${js}\n</script>`);
}

writeFileSync(embeddedPath, html);

console.log("Wrote", embeddedPath);
