//! Embeds the application icon and the `VERSIONINFO` block into the executable's
//! resource section so that Explorer, the taskbar and the file properties dialog
//! show both the icon and the product version for the `.exe`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resource group ID; the shell picks the lowest-numbered icon group for the file icon.
const ICON_RESOURCE_ID: u32 = 1;

/// Numeric resource ID of the `VERSIONINFO` block (1 is the conventional value).
const VERSION_RESOURCE_ID: u32 = 1;

/// Source icon, shared with the window icon embedded by `src/main.rs`.
const ICON_FILE: &str = "assets/RMathTex.ico";

/// Executable file name shown in the properties dialog.
const ORIGINAL_FILENAME: &str = "RMathTex.exe";

/// Internal name reported to the shell; matches the Cargo binary name.
const INTERNAL_NAME: &str = "RMathTex";

/// Product name shown in the properties dialog.
const PRODUCT_NAME: &str = "RMathTex";

/// Copyright notice shown in the properties dialog.
const COPYRIGHT: &str = "Copyright (C) 2026 alwaysrun. Licensed under GPL-3.0-only.";

/// Language identifier (US English) of the string table.
const LANGUAGE_ID: &str = "0x0409";

/// Code page (Unicode) of the string table.
const CODEPAGE_ID: &str = "0x04b0";

fn main() {
    println!("cargo:rerun-if-changed={ICON_FILE}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let script = out_dir.join("r-math-tex.rc");

    // `rc.exe` expands C-style escapes, so a Windows path must double its backslashes.
    let icon_path = manifest_dir.join(ICON_FILE).display().to_string();
    let source = resource_script(&quote(&icon_path), &package_version());
    fs::write(&script, source).expect("failed to write the resource script");

    let compiled = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        compile_with_rc(&script, &out_dir)
    } else {
        compile_with_windres(&script, &out_dir)
    };
    println!("cargo:rustc-link-arg={}", compiled.display());
}

/// Returns the package version declared in `Cargo.toml`.
fn package_version() -> String {
    env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is set by Cargo")
}

/// Builds the resource script holding the icon and the version information.
fn resource_script(icon_path: &str, version: &str) -> String {
    let fixed = fixed_version(version);
    let fixed_quadruple = format!("{},{},{},0", fixed[0], fixed[1], fixed[2]);
    format!(
        "{ICON_RESOURCE_ID} ICON \"{icon_path}\"\n\
         \n\
         {VERSION_RESOURCE_ID} VERSIONINFO\n\
         FILEVERSION {fixed_quadruple}\n\
         PRODUCTVERSION {fixed_quadruple}\n\
         FILEFLAGSMASK 0x3fL\n\
         FILEFLAGS 0x0L\n\
         FILEOS 0x40004L\n\
         FILETYPE 0x1L\n\
         FILESUBTYPE 0x0L\n\
         BEGIN\n\
         BLOCK \"StringFileInfo\"\n\
         BEGIN\n\
         BLOCK \"040904b0\"\n\
         BEGIN\n\
         VALUE \"FileDescription\", \"{description}\"\n\
         VALUE \"FileVersion\", \"{version}\"\n\
         VALUE \"InternalName\", \"{INTERNAL_NAME}\"\n\
         VALUE \"LegalCopyright\", \"{COPYRIGHT}\"\n\
         VALUE \"OriginalFilename\", \"{ORIGINAL_FILENAME}\"\n\
         VALUE \"ProductName\", \"{PRODUCT_NAME}\"\n\
         VALUE \"ProductVersion\", \"{version}\"\n\
         END\n\
         END\n\
         BLOCK \"VarFileInfo\"\n\
         BEGIN\n\
         VALUE \"Translation\", {LANGUAGE_ID}, {CODEPAGE_ID}\n\
         END\n\
         END\n",
        description = quote(&env::var("CARGO_PKG_DESCRIPTION").unwrap_or_default()),
    )
}

/// Splits `major.minor.patch` into the fixed-point quadruple used by `VERSIONINFO`.
fn fixed_version(version: &str) -> [u16; 3] {
    let core = version
        .split(['-', '+'])
        .next()
        .expect("a version string always has a leading component");
    let mut fixed = [0u16; 3];
    for (index, slot) in fixed.iter_mut().enumerate() {
        let piece = core.split('.').nth(index).unwrap_or("0");
        *slot = piece.parse().unwrap_or_else(|_| {
            panic!(
                "package version {version:?}: component {piece:?} is not a 16-bit unsigned integer"
            )
        });
    }
    fixed
}

/// Escapes a value so that it survives the C-style parsing performed by `rc.exe`.
fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Compiles the resource script with the Windows SDK `rc.exe` (MSVC toolchain).
fn compile_with_rc(script: &Path, out_dir: &Path) -> PathBuf {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH is set by Cargo");
    let mut rc = cc::windows_registry::find(&arch, "rc.exe").unwrap_or_else(|| {
        panic!("rc.exe from a Windows SDK installation is required to embed {ICON_FILE}")
    });
    let resource = out_dir.join("r-math-tex.res");
    let status = rc
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg(script)
        .status()
        .expect("failed to start rc.exe");
    assert!(
        status.success(),
        "rc.exe failed to compile the executable resource section: {status}"
    );
    resource
}

/// Compiles the resource script with `windres` (GNU toolchain).
fn compile_with_windres(script: &Path, out_dir: &Path) -> PathBuf {
    let object = out_dir.join("r-math-tex.o");
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
        "windres failed to compile the executable resource section: {status}"
    );
    object
}
