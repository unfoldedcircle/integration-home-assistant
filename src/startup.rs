// Copyright (c) 2024 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use const_format::formatcp;

/// Build information like timestamp, git hash, etc.
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// Application version built from git version information.
pub const APP_VERSION: &str = formatcp!(
    "{}{}",
    match built_info::GIT_VERSION {
        Some(v) => v,
        None => formatcp!("{}-non-git", built_info::PKG_VERSION),
    },
    match built_info::GIT_DIRTY {
        Some(_) => "-dirty",
        None => "",
    }
);
