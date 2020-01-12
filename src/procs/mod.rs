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

pub use ipc::Ipc;
pub use leader::leader;
pub use logger::Logger;
pub use supervisor::supervisor;

use std::process::Stdio;

use async_trait::async_trait;
use clap::App;

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::pipe::End;

pub const CONTROL_IN: &str = "control-in";
pub const INIT: &str = "init";

#[async_trait]
pub trait Process: Send + 'static {
    const NAME: &'static str;
    type Direction: End;

    fn sub_command() -> App<'static, 'static>;

    async fn run(control: AsyncCtlEnd<Self::Direction>);

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            stdin: Stdio::piped(),
            stderr: Stdio::piped(),
            stdout: Stdio::piped(),
        }
    }
}
