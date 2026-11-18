//! The build of the bridge of the FFI: the shim of the stack of Perl.
//!
//! The C code includes `perl.h` of the system and it links the interpreter. The
//! build of the port asks the system for the two sets of flags with
//! `perl -MExtUtils::Embed`, the port of the `--with-perl` of `configure`, and
//! it builds `src/shim.c` with the compiler of the system.

use std::process::Command;

/// Runs a command and answers its output.
fn run(program: &str, args: &[&str]) -> String {
    let out = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {program}: {e}"));
    assert!(out.status.success(), "{program} failed");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Runs the perl of the system for a set of flags.
fn perl(args: &[&str]) -> Vec<String> {
    run("perl", args)
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=src/shim.c");
    println!("cargo:rerun-if-changed=build.rs");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let out = std::env::var("OUT_DIR").unwrap_or_default();
    let object = format!("{out}/shim.o");
    let archive = format!("{out}/libbinkd_perl_shim.a");

    // The flags of the system: the two words of `-iwithsysroot <dir>` of macOS
    // hold the directory of the headers of Perl inside the SDK, so the build joins
    // the two words into one argument, the port of the flag of the C compiler.
    let mut compile = Command::new(&cc);
    compile.arg("-c").arg("-o").arg(&object).arg("src/shim.c");
    // The directory of the SDK: the flag `-iwithsysroot` of the Perl of macOS
    // holds a path inside the SDK, so the compiler needs the root of the two, the
    // port of the build of the C code of the system.
    if let Ok(sdk) = std::process::Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
    {
        if sdk.status.success() {
            let sdk = String::from_utf8_lossy(&sdk.stdout).trim().to_string();
            if !sdk.is_empty() {
                compile.arg(format!("-isysroot{sdk}"));
            }
        }
    }
    let flags = perl(&["-MExtUtils::Embed", "-e", "ccopts"]);
    let mut i = 0;
    while i < flags.len() {
        if flags[i] == "-iwithsysroot" && i + 1 < flags.len() {
            compile.arg(format!("-iwithsysroot{}", flags[i + 1]));
            i += 2;
            continue;
        }
        compile.arg(&flags[i]);
        i += 1;
    }
    let status = compile.status().expect("cannot run the compiler");
    assert!(status.success(), "the compiler of the shim failed");

    let status = Command::new("ar")
        .args(["crs", &archive, &object])
        .status()
        .expect("cannot run ar");
    assert!(status.success(), "ar failed");

    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=binkd_perl_shim");
    for flag in perl(&["-MExtUtils::Embed", "-e", "ldopts"]) {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        } else if let Some(path) = flag.strip_prefix("-Wl,-rpath,") {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
        }
    }
}
