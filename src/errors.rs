// Copyright 2018 The Rust Project Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Custom errors for cargo-bisect-rustc

use std::fmt;
use std::io;

use failure::Fail;

use super::ToolchainSpec;

#[derive(Fail, Debug)]
pub(crate) enum ArchiveError {
    #[fail(display = "Failed to parse archive: {}", _0)]
    Archive(#[cause] io::Error),
    #[fail(display = "Failed to create directory: {}", _0)]
    CreateDir(#[cause] io::Error),
}

#[derive(Fail, Debug)]
#[fail(display = "will never happen")]
pub(crate) struct BoundParseError {}

#[derive(Fail, Debug)]
pub(crate) enum DownloadError {
    #[fail(display = "Tarball not found at {}", _0)]
    NotFound(String),
    #[fail(display = "A reqwest error occurred: {}", _0)]
    Reqwest(#[cause] reqwest::Error),
    #[fail(display = "An archive error occurred: {}", _0)]
    Archive(#[cause] ArchiveError),
}

#[derive(Debug, Fail)]
#[fail(display = "exiting with {}", _0)]
pub(crate) struct ExitStatusError(pub(crate) i32);

#[derive(Fail, Debug)]
pub(crate) enum InstallError {
    #[fail(display = "Could not find {}; url: {}", spec, url)]
    NotFound { url: String, spec: ToolchainSpec },
    #[fail(display = "Could not download toolchain: {}", _0)]
    Download(#[cause] DownloadError),
    #[fail(display = "Could not create tempdir: {}", _0)]
    TempDir(#[cause] io::Error),
    #[fail(display = "Could not move tempdir into destination: {}", _0)]
    Move(#[cause] io::Error),
}
