//! Embeds the application icon into the executable's resource section so that
//! Explorer, the taskbar and the file properties dialog show it for the `.exe`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resource group ID; the shell picks the lowest-numbered icon group for the file icon.
const ICON_RESOURCE_ID: u32 = 1;

/// Source icon, shared with the window icon embedded by `src/main.rs`.
const ICON_FILE: &str = "assets/RMathTex.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON_FILE}");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let script = out_dir.join("r-math-tex-icon.rc");

    // `rc.exe` expands C-style escapes, so a Windows path must double its backslashes.
    let icon_path = manifest_dir.join(ICON_FILE).display().to_string();
    let source = format!(
        "{ICON_RESOURCE_ID} ICON \"{}\"\n",
        icon_path.replace('\\', "\\\\")
    );
    fs::write(&script, source).expect("failed to write the icon resource script");

    let compiled = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        compile_with_rc(&script, &out_dir)
    } else {
        compile_with_windres(&script, &out_dir)
    };
    println!("cargo:rustc-link-arg={}", compiled.display());
}

/// Compiles the resource script with the Windows SDK `rc.exe` (MSVC toolchain).
fn compile_with_rc(script: &Path, out_dir: &Path) -> PathBuf {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH is set by Cargo");
    let mut rc = cc::windows_registry::find(&arch, "rc.exe").unwrap_or_else(|| {
        panic!("rc.exe from a Windows SDK installation is required to embed {ICON_FILE}")
    });
    let resource = out_dir.join("r-math-tex-icon.res");
    let status = rc
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg(script)
        .status()
        .expect("failed to start rc.exe");
    assert!(
        status.success(),
        "rc.exe failed to compile {ICON_FILE} into the executable resource section: {status}"
    );
    resource
}

/// Compiles the resource script with `windres` (GNU toolchain).
fn compile_with_windres(script: &Path, out_dir: &Path) -> PathBuf {
    let object = out_dir.join("r-math-tex-icon.o");
    let status = Command::new("windres")
        .arg("--input")
        .arg(script)
        .arg("--output")
        .arg(&object)
        .arg("--output-format=coff")
        .status()
        .expect("failed to start windres; the GNU toolchain needs it to embed the icon");
    assert!(
        status.success(),
        "windres failed to compile {ICON_FILE} into the executable resource section: {status}"
    );
    object
}
