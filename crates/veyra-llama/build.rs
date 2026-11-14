// Links the pinned llama.cpp static libraries and compiles the C++ shim against its headers.
//   VEYRA_LLAMA_DIR=<dir>  with  <dir>/include/{llama.h,ggml*.h}  and  <dir>/lib/*.a|*.lib   (produced by scripts/build-llama.sh)
// Without it the crate builds as a STUB (`cfg(veyra_stub)`): load() returns an explicit error. This keeps `cargo test` working on machines that
// do not have the native libraries, while the CI build of the application always sets the variable and fails if the libraries are missing.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=VEYRA_LLAMA_DIR");
    println!("cargo:rerun-if-changed=csrc/veyra_llama.cpp");
    println!("cargo:rerun-if-changed=csrc/veyra_llama.h");
    println!("cargo:rustc-check-cfg=cfg(veyra_stub)");
    let Some(dir) = std::env::var_os("VEYRA_LLAMA_DIR").map(PathBuf::from) else {
        println!("cargo:warning=VEYRA_LLAMA_DIR not set: building veyra-llama as a stub (no local inference)");
        println!("cargo:rustc-cfg=veyra_stub");
        return;
    };
    let (inc, lib) = (dir.join("include"), dir.join("lib"));
    assert!(inc.join("llama.h").exists(), "VEYRA_LLAMA_DIR/include/llama.h not found: {}", inc.display());
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").map(|e| e == "msvc").unwrap_or(false);
    let mut b = cc::Build::new();
    b.cpp(true).file("csrc/veyra_llama.cpp").include(&inc).include("csrc");
    if msvc { b.flag("/std:c++17").flag("/EHsc"); } else { b.flag("-std=c++17"); }
    b.compile("veyra_llama_shim");
    println!("cargo:rustc-link-search=native={}", lib.display());
    // dependency order matters for static linking: llama -> ggml -> backends -> ggml-base
    let present = |n: &str| lib.join(format!("lib{n}.a")).exists() || lib.join(format!("{n}.lib")).exists();
    let mut libs: Vec<&str> = vec!["llama", "ggml"];
    for n in ["ggml-metal", "ggml-blas", "ggml-cpu"] { if present(n) { libs.push(n); } }
    libs.push("ggml-base");
    for n in libs { assert!(present(n), "static library {n} not found in {}", lib.display()); println!("cargo:rustc-link-lib=static={n}"); }
    if target_os == "macos" {
        for f in ["Accelerate", "Foundation", "Metal", "MetalKit"] { println!("cargo:rustc-link-lib=framework={f}"); }
        println!("cargo:rustc-link-lib=c++");
        // Objective-C `@available(...)` checks in ggml-metal call __isPlatformVersionAtLeast, which lives in the compiler runtime, not in libSystem.
        if let Ok(o) = std::process::Command::new("clang").arg("--print-resource-dir").output() { let dir = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let d = std::path::Path::new(&dir).join("lib").join("darwin"); if d.join("libclang_rt.osx.a").exists() { println!("cargo:rustc-link-search=native={}", d.display()); println!("cargo:rustc-link-lib=static=clang_rt.osx"); } }
    } else if target_os == "windows" { println!("cargo:rustc-link-lib=advapi32"); // ggml-cpu reads the CPU name from the registry
    } else if target_os == "linux" { println!("cargo:rustc-link-lib=dl"); println!("cargo:rustc-link-lib=pthread"); println!("cargo:rustc-link-lib=stdc++"); }
}
