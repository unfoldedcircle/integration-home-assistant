// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

pub fn my_ipv4_interfaces() -> Vec<if_addrs::IfAddr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| {
            if i.is_loopback() {
                None
            } else {
                match i.addr {
                    if_addrs::IfAddr::V4(_) => Some(i.addr),
                    _ => None,
                }
            }
        })
        .collect()
}
