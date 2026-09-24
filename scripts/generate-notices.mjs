// Generates THIRD_PARTY_NOTICES.txt (repo root) and app/public/THIRD_PARTY_NOTICES.txt (shown in
// Settings → About), covering everything that ships in the app:
//   1. Rust crates, via cargo-about (about.toml / about.hbs) — fails on a non-allowed license
//   2. native code compiled into llama.cpp (read from the llama-cpp-sys-2 sources)
//   3. npm packages bundled into the UI
//   4. Android libraries (Apache-2.0) and Jackson's NOTICE
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

// 4. Android libraries
add(
  "Android app (AndroidX, Material Components, Kotlin, kotlinx.coroutines, Jackson) — Apache-2.0",
  `The Android build includes libraries by Google, JetBrains and FasterXML under the Apache
License 2.0 (full text above, under Rust crates). Jackson NOTICE:

Jackson is a high-performance JSON processor (parser, generator). Jackson is licensed under
the Apache License 2.0. Jackson core includes FastDoubleParser (MIT, Copyright (c) 2023 Werner
Randelshofer) and bigint (BSD-2-Clause, Copyright 2020 Tim Buktu).`,
);

// 5. Microsoft WebView2 loader, statically linked on Windows by webview2-com-sys
const tmp = mkdtempSync(join(tmpdir(), "webview2-"));
const nupkg = join(tmp, "webview2.zip");
execFileSync("curl", ["-sSL", "-o", nupkg, "https://www.nuget.org/api/v2/package/Microsoft.Web.WebView2"]);
// A .nupkg is a zip; Python's zipfile reads it on every OS (GNU tar can't).
const python = ["python3", "python", "py"].find((p) => {
  try {
    execFileSync(p, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
});
const webview2License = execFileSync(python, ["-c", "import sys, zipfile; sys.stdout.write(zipfile.ZipFile(sys.argv[1]).read('LICENSE.txt').decode())", nupkg], { encoding: "utf8" });
add("Microsoft Edge WebView2 SDK loader (Windows builds)", webview2License);

const out = `${rust.trim()}\n\nOther components\n----------------\n\n${sections.join("\n")}`;
writeFileSync(join(root, "THIRD_PARTY_NOTICES.txt"), out);
writeFileSync(join(root, "app", "public", "THIRD_PARTY_NOTICES.txt"), out);
console.log(`wrote THIRD_PARTY_NOTICES.txt (${Math.round(out.length / 1024)} KB)`);
