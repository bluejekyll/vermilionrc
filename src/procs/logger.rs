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
use crate::procs::{CtlIn, HasCtlIn, HasCtlOut, NoCtlOut, Process};
use crate::Error;

/// Recv stdout file descriptors to poll and stdout data from.
///
/// Rules:
///  - may listen for new file descriptors on pipe from ipc
///  - may channel those file descriptors to stdout
#[derive(Debug)]
pub struct Logger;

#[async_trait]
impl Process<HasCtlIn, NoCtlOut> for Logger {
    const NAME: &'static str = "logger";

    fn inner_sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME)
            .about("Common logger for all processes under vermilion management")
    }

    async fn run(self, args: &ArgMatches<'_>) -> Result<(), Error> {
        println!("Logger: started");
        let mut control = Self::get_ctl_in(args)
            .expect("failed to get control-in")
            .expect("control-in parameter not present");

        loop {
            let msg = Message::recv_msg(&mut control).await;

            let mut msg: Message = match msg {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!("Logger: error receiving message");
                    continue;
                }
            };

            let pid = msg.metadata().pid().unwrap_or(-1);
            let name = msg.metadata().proc_name().unwrap_or("unknown").to_string();
            let reader = msg.take_read_pipe_end().expect("take_pipe fails");

            // ok we got a file descriptor. Now we will spawn a background task to listen for log messages from it
            println!("Logger: received fd from: {}", pid);
            tokio::spawn(print_messages_to_stdout(reader, pid, name));
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

async fn print_messages_to_stdout(reader: AsyncPipeEnd<Read>, pid: libc::pid_t, proc_name: String) {
    use tokio::io::ErrorKind;
    let mut lines = BufReader::with_capacity(1_024, reader).lines();

    // read until EOF, or there's an error
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => println!("LOG:{}[{}]: {}", proc_name, pid, line),
            Ok(None) => break,
            Err(e) => match e.kind() {
                // something odd here...
                ErrorKind::WouldBlock => println!("LOG: WOULD_BLOCK"),
                _ => eprintln!("{} LOG: error reading from pipe: {}", pid, e),
            },
        }
    }

    println!("{} LOGGING SHUTDOWN", pid);
}
