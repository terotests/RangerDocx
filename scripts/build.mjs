#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Build the RangerDocx editor with its EVGUI interface (shell/DocsApp.rgr):
 * one compiled engine, the page, the renderer and the fonts — static files any
 * server can serve.
 *
 *   node scripts/build.mjs [--ranger DIR] [--evgui DIR] [--evgsheets DIR] [--out DIR] [--no-minify]
 *
 * The sources import each other as if they sat side by side in a Ranger
 * checkout (gallery/docx_viewer, gallery/evgui, gallery/evgsheets), so the
 * build assembles that tree under .stage/ out of links: everything from the
 * Ranger checkout, with this repository, EVGUI and EVGSheets in their places.
 * The Ranger checkout is not written to.
 *
 *   Ranger     --ranger, $RANGER_DIR, or ../Ranger. Its lib/ and gallery/ must
 *              be fetched already (npm run deps in it).
 *   EVGUI      --evgui, $EVGUI_DIR, ../EVGUI, or cloned into .deps/EVGUI.
 *   EVGSheets  --evgsheets, $EVGSHEETS_DIR, ../EVGSheets, or cloned into
 *              .deps/EVGSheets. Only its RibbonCtl is used.
 *
 * Output (default dist/): index.html, docs.mjs, docx_shell.js, docs.css, gl/,
 * fonts/, document.docx.
 */
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const argv = process.argv.slice(2);
const flag = (name) => {
  const at = argv.indexOf(name);
  return at >= 0 ? argv[at + 1] : undefined;
};
const die = (msg) => {
  console.error(msg);
  process.exit(1);
};
const run = (cmd, args, opts = {}) => {
  const r = spawnSync(cmd, args, { stdio: "inherit", ...opts });
  if (r.status !== 0) die(`${cmd} ${args.join(" ")} failed`);
};

const RANGER = path.resolve(flag("--ranger") || process.env.RANGER_DIR || path.join(REPO, "..", "Ranger"));
const OUT = path.resolve(flag("--out") || path.join(REPO, "dist"));
const minify = !argv.includes("--no-minify");
if (!fs.existsSync(path.join(RANGER, "dist", "rgrc.js"))) {
  die(`not a Ranger checkout: ${RANGER} (pass --ranger <dir> or set RANGER_DIR)`);
}
if (!fs.existsSync(path.join(RANGER, "lib", "evg", "EVGElement.rgr"))) {
  die(`${RANGER}/lib/evg is missing: run "npm run deps" in the Ranger checkout first`);
}

/** A sibling repository: the flag, the variable, a checkout beside this one, or a clone. */
function sibling(flagName, envName, repoName, probe) {
  let dir = flag(flagName) || process.env[envName];
  if (!dir) {
    const candidates = [path.join(REPO, "..", repoName), path.join(REPO, ".deps", repoName)];
    dir = candidates.find((d) => fs.existsSync(path.join(d, probe)));
  }
  if (!dir) {
    dir = path.join(REPO, ".deps", repoName);
    const ref = process.env[envName.replace(/_DIR$/, "_REF")] || "main";
    run("git", ["clone", "--depth", "1", "--branch", ref, `https://github.com/terotests/${repoName}`, dir]);
  }
  return path.resolve(dir);
}
const EVGUI = sibling("--evgui", "EVGUI_DIR", "EVGUI", "src/UiHost.rgr");
const EVGSHEETS = sibling("--evgsheets", "EVGSHEETS_DIR", "EVGSheets", "src/RibbonCtl.rgr");

// --- the build tree ------------------------------------------------------------
const STAGE = path.join(REPO, ".stage");
const OWN = { docx_viewer: REPO, evgui: EVGUI, evgsheets: EVGSHEETS };
fs.rmSync(STAGE, { recursive: true, force: true });
fs.mkdirSync(path.join(STAGE, "gallery"), { recursive: true });
const link = (to, at) => fs.symlinkSync(to, at, fs.statSync(to).isDirectory() ? "dir" : "file");
for (const e of fs.readdirSync(RANGER)) {
  if (e === "gallery" || e === ".git" || e === "node_modules") continue;
  link(path.join(RANGER, e), path.join(STAGE, e));
}
for (const e of fs.readdirSync(path.join(RANGER, "gallery"))) {
  if (e in OWN) continue;
  link(path.join(RANGER, "gallery", e), path.join(STAGE, "gallery", e));
}
for (const [name, dir] of Object.entries(OWN)) link(dir, path.join(STAGE, "gallery", name));

// --- the shell's tests (--test): compiled in the stage tree and run there ---------
if (argv.includes("--test")) {
  const outDir = path.join(STAGE, ".bin");
  fs.mkdirSync(outDir, { recursive: true });
  const t = spawnSync(
    process.execPath,
    ["dist/rgrc.js", "-es6", "gallery/docx_viewer/tests/DocsAppTest.rgr", `-d=${outDir}`, "-o=DocsAppTest.js", "-nodecli"],
    { cwd: STAGE, env: { ...process.env, RANGER_LIB: "./compiler/Lang.rgr:./lib/stdops.rgr" }, encoding: "utf8", maxBuffer: 1 << 28 },
  );
  const tlog = (t.stdout || "") + (t.stderr || "");
  const js = path.join(outDir, "DocsAppTest.js");
  if (/Compilation FAILED/.test(tlog) || !fs.existsSync(js)) {
    console.error(tlog.split("\n").filter((l) => /FAIL|│|\^|error/i.test(l)).slice(0, 80).join("\n"));
    die("could not compile gallery/docx_viewer/tests/DocsAppTest.rgr");
  }
  const res = spawnSync(process.execPath, [js], { cwd: STAGE, encoding: "utf8", maxBuffer: 1 << 28 });
  const out = (res.stdout || "") + (res.stderr || "");
  process.stdout.write(out);
  if (res.status !== 0 || !/ALL PASS/.test(out)) die("DocsAppTest failed");
  process.exit(0);
}

// --- compile --------------------------------------------------------------------
fs.mkdirSync(OUT, { recursive: true });
const bundle = path.join(OUT, "docx_shell.js");
fs.rmSync(bundle, { force: true });
const t0 = Date.now();
const r = spawnSync(
  process.execPath,
  ["dist/rgrc.js", "-es6", "gallery/docx_viewer/shell/DocsApp.rgr", `-d=${OUT}`, "-o=docx_shell.js"],
  { cwd: STAGE, env: { ...process.env, RANGER_LIB: "./compiler/Lang.rgr:./lib/stdops.rgr" }, encoding: "utf8", maxBuffer: 1 << 28 },
);
const log = (r.stdout || "") + (r.stderr || "");
if (/Compilation FAILED/.test(log) || !fs.existsSync(bundle)) {
  console.error(log.split("\n").filter((l) => /FAIL|│|\^|error/i.test(l)).slice(0, 80).join("\n"));
  die("could not compile gallery/docx_viewer/shell/DocsApp.rgr");
}
console.log(`  compiled in ${((Date.now() - t0) / 1000).toFixed(1)}s`);

// One global, so the page and other scripts on it do not collide. The measurer
// classes go out with the app: the canvas text measurer is installed into THIS
// bundle's copy of them.
let src = fs.readFileSync(bundle, "utf8");
src =
  "// RangerDocx engine — compiled from Ranger source (github.com/terotests/RangerDocx).\n" +
  "(function () {\n" + src +
  "\n;globalThis.RangerDocxModule = { DocsApp: DocsApp, EVGHostTextMeasurer: EVGHostTextMeasurer, EVGDefaultMeasurer: EVGDefaultMeasurer };\n})();\n";
fs.writeFileSync(bundle, src);

// Loadable by a browser? Nothing may reach for the file system while loading.
{
  const probe = new Function("globalThis", "require", src + "\n;return typeof globalThis.RangerDocxModule.DocsApp;");
  if (probe({}, undefined) !== "function") die("docx_shell.js does not publish RangerDocxModule.DocsApp");
}

if (minify) {
  let esbuild = null;
  for (const from of [REPO, RANGER, path.join(RANGER, "gallery", "ui", "conformance", "dom")]) {
    try {
      esbuild = createRequire(path.join(from, "package.json"))("esbuild");
      break;
    } catch (_) {
      /* next */
    }
  }
  if (esbuild) {
    const before = fs.statSync(bundle).size;
    const out = esbuild.transformSync(fs.readFileSync(bundle, "utf8"), { minify: true, target: "es2020" });
    fs.writeFileSync(bundle, out.code);
    console.log(`  minified ${(before / 1e6).toFixed(2)} MB → ${(out.code.length / 1e6).toFixed(2)} MB`);
  } else {
    console.log("  (no esbuild: shipping the unminified engine)");
  }
}

// --- the page and its assets ------------------------------------------------------
for (const f of ["index.html", "docs.mjs"]) fs.copyFileSync(path.join(REPO, "web", "docs", f), path.join(OUT, f));
fs.copyFileSync(path.join(REPO, "shell", "docs.css"), path.join(OUT, "docs.css"));
fs.mkdirSync(path.join(OUT, "gl"), { recursive: true });
for (const f of ["evg-webgl.js", "evg-a11y.js", "evg-measure.js"]) {
  fs.copyFileSync(path.join(RANGER, "lib", "evg", "gl", f), path.join(OUT, "gl", f));
}
const FONT_SRC = path.join(RANGER, "gallery", "pdf_writer", "assets", "fonts");
const FONTS = [
  "Open_Sans/OpenSans-Regular.ttf", "Open_Sans/OpenSans-Bold.ttf", "Open_Sans/OpenSans-Italic.ttf", "Open_Sans/OpenSans-BoldItalic.ttf",
  "Noto_Sans/NotoSans-Regular.ttf", "Noto_Emoji/NotoEmoji-Regular.ttf",
  "El_Messiri/ElMessiri-Regular.ttf", "El_Messiri/ElMessiri-Bold.ttf",
];
fs.mkdirSync(path.join(OUT, "fonts"), { recursive: true });
for (const f of FONTS) fs.copyFileSync(path.join(FONT_SRC, f), path.join(OUT, "fonts", path.basename(f)));
fs.copyFileSync(path.join(REPO, "fixtures", "20-business-report.docx"), path.join(OUT, "document.docx"));
fs.writeFileSync(path.join(OUT, ".nojekyll"), "");

// --- the build stamp ----------------------------------------------------------------
// Every URL the page loads carries ?v=<hash>, so a rebuilt page is fetched rather
// than taken from a cache that still holds the last one.
const h = crypto.createHash("sha1");
for (const f of ["docx_shell.js", "docs.mjs", "docs.css", "gl/evg-webgl.js", "gl/evg-a11y.js", "gl/evg-measure.js"]) {
  h.update(fs.readFileSync(path.join(OUT, f)));
}
const STAMP = h.digest("hex").slice(0, 10);
const page = path.join(OUT, "index.html");
fs.writeFileSync(page, fs.readFileSync(page, "utf8").split("__BUILD__").join(STAMP));
console.log(`  ${OUT}`);
console.log(`build ${STAMP}`);
