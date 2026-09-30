fn main() {
    #[cfg(target_os = "macos")]
    link_swift_runtime();
    tauri_build::build()
}

/// The ScreenCaptureKit bridge is Swift and needs the Swift compatibility
/// libs. Resolve them from the active toolchain (Xcode or Command Line Tools).
#[cfg(target_os = "macos")]
fn link_swift_runtime() {
    let Ok(out) = std::process::Command::new("xcrun").args(["--find", "swift"]).output() else {
        return;
    };
    let swift = String::from_utf8_lossy(&out.stdout);
    let Some(bin) = std::path::Path::new(swift.trim()).parent() else {
        return;
    };
    let lib = bin.join("../lib/swift/macosx");
    if lib.exists() {
        println!("cargo:rustc-link-search=native={}", lib.display());
    }
}
