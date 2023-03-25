// Copyright (c) 2023 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

macro_rules! return_fut_ok {
    ($result:expr) => {
        return Box::pin(fut::result(Ok($result)));
    };
}

macro_rules! return_fut_err {
    ($result:expr) => {
        return Box::pin(fut::result(Err($result)));
    };
}

pub(crate) use return_fut_err;
pub(crate) use return_fut_ok;
