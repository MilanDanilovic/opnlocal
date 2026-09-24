// Generates THIRD_PARTY_NOTICES.txt (repo root) and app/public/THIRD_PARTY_NOTICES.txt (shown in
// Settings → About), covering everything that ships in the app:
//   1. Rust crates, via cargo-about (about.toml / about.hbs) — fails on a non-allowed license
//   2. native code compiled into llama.cpp (read from the llama-cpp-sys-2 sources)
//   3. npm packages bundled into the UI
//   4. Android libraries (Apache-2.0), with Jackson's NOTICE files from its jar
//   5. Microsoft WebView2 loader (Windows), license from the official NuGet package
// Usage: node scripts/generate-notices.mjs   (needs `cargo install cargo-about`)
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sections = [];
const rule = "-".repeat(96);
const add = (title, text) => sections.push(`${rule}\n${title}\n${rule}\n${text.trim()}\n`);

// 1. Rust crates
const rustFile = join(mkdtempSync(join(tmpdir(), "notices-")), "rust.txt");
execFileSync("cargo", ["about", "generate", "--fail", "-o", rustFile, "about.hbs"], { cwd: root, stdio: ["ignore", "ignore", "inherit"] });
const rust = readFileSync(rustFile, "utf8");

// 2. Native code inside llama.cpp
const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1"], { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 }));
const sys = meta.packages.find((p) => p.name === "llama-cpp-sys-2");
const llama = join(dirname(sys.manifest_path), "llama.cpp");
add("llama.cpp / ggml (MIT)", readFileSync(join(llama, "LICENSE"), "utf8"));
const headerLicense = (file, marker) => {
  const text = readFileSync(file, "utf8");
  const start = text.indexOf(marker);
  return text.slice(start, text.indexOf("*/", start) > 0 ? text.indexOf("*/", start) : start + 1500);
};
for (const [title, rel, marker] of [
  ["nlohmann/json (MIT) — used by llama.cpp's chat templates", "vendor/nlohmann/json.hpp", "SPDX"],
  ["cpp-httplib (MIT) — compiled into llama.cpp's common library (unused by opnlocal)", "vendor/cpp-httplib/httplib.h", "Copyright"],
  ["llamafile sgemm (MIT) — ggml CPU kernels", "ggml/src/ggml-cpu/llamafile/sgemm.cpp", "Copyright"],
]) {
  const file = join(llama, rel);
  if (existsSync(file)) add(title, headerLicense(file, marker));
}

// 3. npm packages bundled into the UI
const pkgs = ["@tauri-apps/api", "@tauri-apps/plugin-dialog", "@tauri-apps/plugin-opener", "@tauri-apps/plugin-fs", "dompurify", "highlight.js", "marked", "svelte", "clsx", "esm-env"];
for (const name of pkgs) {
  const dir = join(root, "app", "node_modules", name);
  if (!existsSync(dir)) continue;
  const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const file = readdirSync(dir).find((f) => /^(license|licence)/i.test(f));
  add(`${name} ${pkg.version} (${pkg.license})${name === "dompurify" ? " — used under Apache-2.0" : ""}`, file ? readFileSync(join(dir, file), "utf8") : `License: ${pkg.license}`);
}

// Downloads (curl) and zip reading (Python's zipfile, which reads a .jar or .nupkg on every OS;
// GNU tar can't). Raw bytes go to stdout so non-ASCII text survives Windows' console encoding.
const tmp = mkdtempSync(join(tmpdir(), "notices-dl-"));
const download = (url, name) => {
  const file = join(tmp, name);
  execFileSync("curl", ["-sSfL", "-o", file, url]);
  return file;
};
const python = ["python3", "python", "py"].find((p) => {
  try {
    execFileSync(p, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
});
const unzipText = (zip, entry) =>
  execFileSync(python, ["-c", "import sys, zipfile; sys.stdout.buffer.write(zipfile.ZipFile(sys.argv[1]).read(sys.argv[2]))", zip, entry], { encoding: "utf8" });

// 4. Android libraries
add(
  "Android app (AndroidX, Material Components, Kotlin, kotlinx.coroutines) — Apache-2.0",
  "The Android build includes libraries by Google and JetBrains under the Apache License 2.0\n(full text above, under Rust crates).",
);
// Jackson comes with Tauri's Android library. jackson-core's NOTICE files are reproduced as its jar
// ships them; its NOTICE contains the (identical) NOTICE of jackson-databind and jackson-annotations.
const tauri = meta.packages.find((p) => p.name === "tauri");
const jackson = readFileSync(join(dirname(tauri.manifest_path), "mobile", "android", "build.gradle.kts"), "utf8").match(/jackson-databind:([\d.]+)/)[1];
const jacksonCore = download(`https://repo1.maven.org/maven2/com/fasterxml/jackson/core/jackson-core/${jackson}/jackson-core-${jackson}.jar`, "jackson-core.jar");
add(
  `Jackson ${jackson} (jackson-databind, jackson-core, jackson-annotations) — Apache-2.0`,
  ["NOTICE", "FastDoubleParser-NOTICE", "FastDoubleParser-LICENSE", "bigint-LICENSE"]
    .map((f) => `${f}:\n\n${unzipText(jacksonCore, `META-INF/${f}`).trim()}`)
    .join("\n\n"),
);

// 5. Microsoft WebView2 loader, statically linked on Windows by webview2-com-sys
const webview2 = download("https://www.nuget.org/api/v2/package/Microsoft.Web.WebView2", "webview2.zip");
add("Microsoft Edge WebView2 SDK loader (Windows builds)", unzipText(webview2, "LICENSE.txt"));

const out = `${rust.trim()}\n\nOther components\n----------------\n\n${sections.join("\n")}`;
writeFileSync(join(root, "THIRD_PARTY_NOTICES.txt"), out);
writeFileSync(join(root, "app", "public", "THIRD_PARTY_NOTICES.txt"), out);
console.log(`wrote THIRD_PARTY_NOTICES.txt (${Math.round(out.length / 1024)} KB)`);
