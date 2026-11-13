use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    // 1. Try PATH
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }

    // 2. Try standard Windows Kits locations
    let sdk_roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ];

    for root in &sdk_roots {
        let root_path = Path::new(root);
        if let Ok(entries) = std::fs::read_dir(root_path) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            // Sort to prefer newer SDK versions
            versions.sort();
            for ver in versions.iter().rev() {
                let candidate = ver.join("x64").join("rc.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let manifest = manifest_dir.join("app.manifest");
    let rc_file = manifest_dir.join("app.rc");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let res_file = out_dir.join("app.res");

    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/keycast.ico");

    if let Some(rc_exe) = find_rc_exe() {
        let status = Command::new(&rc_exe)
            .arg("/r")
            .arg(format!("/fo{}", res_file.display()))
            .arg(&rc_file)
            .status();

        if let Ok(s) = status {
            if s.success() {
                println!("cargo:rustc-link-arg={}", res_file.display());
            } else {
                eprintln!("Warning: rc.exe returned non-zero status: {:?}", s);
            }
        } else {
            eprintln!("Warning: failed to execute rc.exe at {:?}", rc_exe);
        }
    } else {
        eprintln!("Warning: rc.exe not found; building without PE version resource.");
    }

    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
