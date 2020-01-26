// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

mod ipc;
mod launcher;
mod leader;
mod logger;
mod supervisor;

pub use ipc::Ipc;
pub use launcher::Launcher;
pub use leader::Leader;
pub use logger::Logger;
pub use supervisor::Supervisor;

use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, ArgMatches};

use crate::control::{AsyncCtlEnd, CtlEnd};
use crate::fork::StdIoConf;
use crate::pipe::{End, Read, Write};

pub const CONTROL_IN: &str = "control-in";
pub const INIT: &str = "init";

/// A trait to define common construction of a process
///
/// ## Type Parameters
///
/// * D - Direction of Control, if the Leader this would be Write, all others would be Read
#[async_trait]
pub trait Process<D: End>: Sized + Send + 'static {
    const NAME: &'static str;

    fn sub_command() -> App<'static, 'static>;

    async fn run(control: AsyncCtlEnd<D>, args: &ArgMatches<'_>);

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            stdin: Stdio::piped(),
            stderr: Stdio::piped(),
            stdout: Stdio::piped(),
        }
    }
}

pub trait FlipControl<E: End> {
    fn flip(read: CtlEnd<Read>, write: CtlEnd<Write>) -> (CtlEnd<E>, CtlEnd<E::Reverse>);
}

/// For read flip is a noop
impl FlipControl<Read> for Read {
    fn flip(read: CtlEnd<Read>, write: CtlEnd<Write>) -> (CtlEnd<Read>, CtlEnd<Write>) {
        (read, write)
    }
}

/// For write flip reverse the control
impl FlipControl<Write> for Write {
    fn flip(read: CtlEnd<Read>, write: CtlEnd<Write>) -> (CtlEnd<Write>, CtlEnd<Read>) {
        (write, read)
    }
}
