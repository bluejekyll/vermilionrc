// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

mod ipc;
mod leader;
mod logger;
mod supervisor;

pub use ipc::ipc;
pub use leader::leader;
pub use logger::Logger;
pub use supervisor::supervisor;

use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, SubCommand};

use crate::control::CtlEnd;
use crate::fork::StdIoConf;
use crate::pipe::{End, PipeEnd, Read, Write};

pub const CONTROL_IN: &str = "control-in";
pub const INIT: &str = "init";

#[async_trait]
pub trait Process: Send + 'static {
    const NAME: &'static str;
    type Direction: End;

    fn sub_command() -> App<'static, 'static>;

    async fn run(control: CtlEnd<Self::Direction>);

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            stdin: Stdio::piped(),
            stderr: Stdio::piped(),
            stdout: Stdio::piped(),
        }
    }
}
