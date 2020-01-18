// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, SubCommand};
use tokio::io::AsyncReadExt;

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg;
use crate::pipe::Write;
use crate::procs::Process;

/// Issue commands to the Launcher
///
/// Rules:
///   - should be unprivileged
///   - stdin for control
///   - puts messages onto the IPC
#[derive(Debug)]
pub struct Leader;

#[async_trait]
impl Process<Write> for Leader {
    const NAME: &'static str = "leader";

    fn sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME).about("Leader for the VermilionRC framework")
    }

    async fn run(mut control: AsyncCtlEnd<Write>) {
        println!("Leader started");
        eprintln!("Leader seriously started");

        std::process::exit(8);
    }

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            // The leader will capture the primary stdin
            stdin: Stdio::inherit(),
            // the leader will pipe it's output, eventually to the logger
            stderr: Stdio::piped(),
            // the leader will pipe it's output, eventually to the logger
            stdout: Stdio::piped(),
        }
    }
}
