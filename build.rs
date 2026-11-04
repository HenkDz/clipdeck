use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/clipdeck.ico");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let compiler = env::var_os("RC").map(PathBuf::from).unwrap_or_else(|| {
        let sdk = env::var_os("WindowsSdkDir")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK is required"))
                    .join("Windows Kits/10")
            });
        let mut compilers: Vec<_> = fs::read_dir(sdk.join("bin"))
            .expect("Install the Windows SDK with Visual Studio Build Tools")
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("x64/rc.exe"))
            .filter(|path| path.is_file())
            .collect();
        compilers.sort();
        compilers.pop().expect("Windows SDK rc.exe is required")
    });
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numbers = version.replace('.', ",") + ",0";
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("assets/clipdeck.ico")
        .display()
        .to_string()
        .replace('\\', "/");
    let source = out.join("clipdeck.rc");
    fs::write(
        &source,
        format!(
            r#"
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x40004L
FILETYPE 1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Clipdeck\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "clipdeck\0"
      VALUE "OriginalFilename", "clipdeck.exe\0"
      VALUE "ProductName", "Clipdeck\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
        ),
    )
    .expect("Write Windows resources");
    let resource = out.join("clipdeck.res");
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(source)
        .status()
        .expect("Run Windows resource compiler");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg-bin=clipdeck={}", resource.display());
}
