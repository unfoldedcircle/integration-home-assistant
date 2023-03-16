// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

use std::env;
use std::ffi::OsStr;

/// Retrieves a boolean value from the given environment variable.
///
/// The following string values are considered true: `true` or `1`.
///
/// Returns `false` if the variable is not defined or contains an invalid value.
pub fn bool_from_env<K: AsRef<OsStr>>(key: K) -> bool {
    env::var(key)
        .map(|v| v == "true" || v == "1")
        .unwrap_or_default()
}
