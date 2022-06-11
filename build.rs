// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

fn main() {
    let mut opts = built::Options::default();
    opts.set_dependencies(true);

    let src = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dst = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("built.rs");
    built::write_built_file_with_opts(&opts, src.as_ref(), &dst)
        .expect("Failed to acquire build-time information");

    let target = std::env::var("TARGET").unwrap();
    // TODO verify if still required: special RPi 0 Hack for `undefined reference to __atomic_###`
    if target == "arm-unknown-linux-gnueabihf" {
        println!("cargo:rustc-link-lib=dylib=atomic")
    }
}
