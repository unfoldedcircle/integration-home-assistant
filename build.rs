// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn main() {
    // Get git version information
    let git_version = get_git_version();
    let git_dirty = is_git_dirty();

    built::write_built_file().expect("Failed to acquire build-time information");

    // Write additional git info to built.rs file to supplement what built crate provides
    write_git_info(git_version, git_dirty);

    let target = std::env::var("TARGET").unwrap();
    // TODO verify if still required: special RPi 0 Hack for `undefined reference to __atomic_###`
    if target == "arm-unknown-linux-gnueabihf" {
        println!("cargo:rustc-link-lib=dylib=atomic")
    }
}

fn get_git_version() -> Option<String> {
    // First try to get version from git
    if let Ok(output) = Command::new("git")
        .args(["describe", "--match", "v[0-9]*", "--tags", "HEAD"])
        .output()
        && output.status.success()
        && let Ok(version) = String::from_utf8(output.stdout)
    {
        return Some(version.trim().trim_start_matches('v').to_string());
    }

    // Fallback: try to get commit hash only
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
        && let Ok(commit) = String::from_utf8(output.stdout)
    {
        return Some(commit.trim().to_string());
    }

    None
}

fn is_git_dirty() -> bool {
    // Check if there are any uncommitted changes
    if let Ok(output) = Command::new("git")
        .args(["diff-index", "--name-only", "HEAD", "--"])
        .output()
        && output.status.success()
    {
        return !output.stdout.is_empty();
    }
    false
}

fn write_git_info(git_version: Option<String>, git_dirty: bool) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("git_built.rs");
    let mut f = File::create(&dest_path).unwrap();

    writeln!(f, "// Git information generated at build time").unwrap();

    if let Some(git_version) = git_version {
        writeln!(
            f,
            "pub const GIT_VERSION: Option<&'static str> = Some(\"{}\");",
            git_version
        )
        .unwrap();
    } else {
        writeln!(f, "pub const GIT_VERSION: Option<&'static str> = None;").unwrap();
    }

    if git_dirty {
        writeln!(f, "pub const GIT_DIRTY: Option<bool> = Some(true);").unwrap();
    } else {
        writeln!(f, "pub const GIT_DIRTY: Option<bool> = None;").unwrap();
    }
}
