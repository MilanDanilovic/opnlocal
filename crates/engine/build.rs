// macOS: ggml's Metal code uses `@available(...)` checks, which need the compiler runtime's
// `__isPlatformVersionAtLeast`. rustc links with -nodefaultlibs, so add clang's runtime library
// explicitly. (iOS apps are linked by Xcode, which includes it automatically.)
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::process::Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
        .expect("xcrun clang is available on macOS");
    let resource_dir = String::from_utf8(out.stdout).expect("utf-8 path");
    println!("cargo:rustc-link-search=native={}/lib/darwin", resource_dir.trim());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
