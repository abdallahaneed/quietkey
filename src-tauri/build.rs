fn main() {
    // whisper.cpp's Metal code uses @available checks. With the macOS 12 deployment target of release builds they
    // become runtime calls to __isPlatformVersionAtLeast, which lives in clang's runtime; rustc does not link it.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        if let Ok(out) = std::process::Command::new("clang").arg("--print-resource-dir").output() {
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("cargo:rustc-link-search=native={dir}/lib/darwin");
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        }
    }
    tauri_build::build()
}
