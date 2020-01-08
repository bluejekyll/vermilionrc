// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::AsRawFd;
use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, ArgMatches, SubCommand};
use nix::unistd::read;
use tokio::io::AsyncReadExt;
use tokio::runtime;

use crate::control::{Control, CtlEnd};
use crate::fork::StdIoConf;
use crate::msg;
use crate::pipe::{PipeEnd, Read, Write};
use crate::procs::Process;

/// Recv stdout file descriptors to poll and stdout data from.
///
/// Rules:
///  - may listen for new file descriptors on pipe from ipc
///  - may channel those file descriptors to stdout
#[derive(Debug)]
pub struct Logger;

#[async_trait]
impl Process for Logger {
    const NAME: &'static str = "logger";
    type Direction = Read;

    fn sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME).about("Logger for the VermilionRC framework")
    }

    async fn run(control: CtlEnd<Read>) {
        println!("Logger started");

        let fd = msg::recv_msg(&control).expect("no msg received");

        println!("received filedescriptor: {:?}", fd);

        let mut reader = fd
            .into_async_pipe_end()
            .expect("could not make async pipe end");

        let mut buf = [0u8; 1024];
        let len = reader.read(&mut buf).await.expect("failed to read");
        let line = String::from_utf8_lossy(&buf[..len]);
        println!("LOG_LINE: {}", line);
    }

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            // we need a new input line
            stdin: Stdio::inherit(),
            // the logger should never send data back to any other process
            stderr: Stdio::inherit(),
            // the logger will initially inherit the parents output stream for logging...
            stdout: Stdio::inherit(),
        }
    }
}