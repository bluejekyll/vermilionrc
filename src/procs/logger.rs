// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, SubCommand};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg;
use crate::pipe::{AsyncPipeEnd, Read};
use crate::procs::Process;

/// Recv stdout file descriptors to poll and stdout data from.
///
/// Rules:
///  - may listen for new file descriptors on pipe from ipc
///  - may channel those file descriptors to stdout
#[derive(Debug)]
pub struct Logger;

#[async_trait]
impl Process<Read> for Logger {
    const NAME: &'static str = "logger";

    fn sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME).about("Logger for the VermilionRC framework")
    }

    async fn run(mut control: AsyncCtlEnd<Read>) {
        println!("Logger: started");

        loop {
            let fd = msg::recv_msg(&mut control).await;
            let fd = match fd {
                Ok(fd) => fd,
                Err(e) => {
                    eprintln!("Logger: error receiving file descriptor");
                    continue;
                }
            };

            // ok we got a file descriptor. Now we will spawn a background task to listen for log messages from it
            eprintln!("Logger: received filedescriptor: {:?}", fd);

            let reader = fd
                .into_async_pipe_end()
                .expect("could not make async pipe end");

            tokio::spawn(print_messages_to_stdout(reader));
        }
    }

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            // we need a new input line
            stdin: Stdio::null(),
            // the logger should never send data back to any other process
            stderr: Stdio::inherit(),
            // the logger will initially inherit the parents output stream for logging...
            stdout: Stdio::inherit(),
        }
    }
}

async fn print_messages_to_stdout(reader: AsyncPipeEnd<Read>) {
    use tokio::io::ErrorKind;
    let mut lines = BufReader::with_capacity(1_024, reader).lines();

    // read until EOF, or there's an error
    loop {
        match lines.next_line().await {
            // FIXME: need the PID, of the process here.
            Ok(Some(line)) => println!("LOG: {}", line),
            Ok(None) => break,
            Err(e) => match e.kind() {
                // something odd here...
                ErrorKind::WouldBlock => println!("LOG: WOULD_BLOCK"),
                _ => eprintln!("LOG: error reading from pipe for pid {}: {}", "?FIXME?", e),
            },
        }
    }

    // FIXME: need a PID here
    println!("LOGGING SHUTDOWN for pid: ?FIXME?");
}
