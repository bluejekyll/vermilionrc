// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Common libray functions for Vermilion

pub mod control;
pub mod fork;
pub mod msg;
pub mod pipe;
// FIXME: rename to proc
mod error;
pub mod procs;

pub use error::Error;
