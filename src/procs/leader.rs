// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, ArgMatches, SubCommand};

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
        SubCommand::with_name(Self::NAME).about("Controller for issuing instructions to Vermilion")
    }

    async fn run(control: AsyncCtlEnd<Write>, args: &ArgMatches<'_>) {
        println!("Leader started");
        eprintln!("Leader seriously started");

        for i in 0..60 {
            println!("{} awaiting input: {}", Leader::NAME, i);
            std::thread::sleep(std::time::Duration::from_secs(1))
        }
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
