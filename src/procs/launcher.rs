// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, ArgMatches, SubCommand};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg::Message;
use crate::pipe::{AsyncPipeEnd, Read};
use crate::procs::{CtlIn, CtlOut, HasCtlIn, HasCtlOut, Process};
use crate::Error;

/// Launch programs
///
/// Rules:
/// - may start processes
/// - only listen to messages from IPC
///   - verify messages are from the leader?
/// - read configuration file for process to launch.
#[derive(Debug)]
pub struct Launcher;

#[async_trait]
impl Process<HasCtlIn, HasCtlOut> for Launcher {
    const NAME: &'static str = "launcher";

    fn inner_sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME).about("Process launcher for Vermilion")
    }

    async fn run(self, args: &ArgMatches<'_>) -> Result<(), Error> {
        // FIXME: register SIGCHLD handler to reap all Zombie processes

        println!("Launcher started");

        for i in 0..60 {
            println!("{} awaiting input: {}", Launcher::NAME, i);
            std::thread::sleep(std::time::Duration::from_secs(1))
        }

        Ok(())
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
