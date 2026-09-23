use std::path::{Path, PathBuf};

fn main() {
    stage_llama_libs();
    tauri_build::build();
}

/// On Windows and Linux llama.cpp is built as shared libraries plus runtime-loaded
/// backends (CPU variants, Vulkan). Copy them next to the executable (so `tauri dev`
/// and tests run) and into `libs/` (bundled as resources by tauri.conf.json).
/// Static builds (macOS, iOS, Android) don't export DEP_LLAMA_BACKENDS_DIR, so this is a no-op there.
fn stage_llama_libs() {
    let Ok(backends) = std::env::var("DEP_LLAMA_BACKENDS_DIR") else {
        return;
    };
    let backends = PathBuf::from(backends);
    let out_root = backends.parent().expect("backends dir has a parent");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let (lib_dir, ext) = if windows { ("bin", "dll") } else { ("lib", "so") };
    if !windows {
        // Find the libs next to the binary (dev) or in the bundle's resource dir
        // (/usr/lib/opnlocal for .deb and inside the AppImage).
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN:$ORIGIN/../lib/opnlocal");
    }

    // OUT_DIR = <target>/<profile>/build/<pkg>-<hash>/out → exe dir is 3 levels up.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let exe_dir = out_dir.ancestors().nth(3).unwrap().to_path_buf();
    let stage = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("libs");
    std::fs::create_dir_all(&stage).unwrap();

    for dir in [out_root.join(lib_dir), backends.clone()] {
        for file in shared_libs(&dir, ext) {
            let name = file.file_name().unwrap();
            for dest in [&exe_dir, &stage] {
                std::fs::copy(&file, dest.join(name)).unwrap();
            }
        }
    }
    println!("cargo:rerun-if-env-changed=DEP_LLAMA_BACKENDS_DIR");
}

fn shared_libs(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        // Linux libs are versioned (libllama.so.0.0.1); keep every .so* file.
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.ends_with(&format!(".{ext}")) || name.contains(&format!(".{ext}."))
        })
        .collect()
}
