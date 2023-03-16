// Copyright (c) 2022 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Common utility functions.

mod from_msg_data;
pub mod json;
mod network;

pub use from_msg_data::DeserializeMsgData;
pub use network::*;
