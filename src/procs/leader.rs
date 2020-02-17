// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::RawFd;
use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, ArgMatches, SubCommand};
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};
use tokio::select;
use tokio::stream::StreamExt;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg;
use crate::pipe::Write;
use crate::procs::supervisor::Status;
use crate::procs::{HasCtlOut, NoCtlIn, Process};
use crate::Error;

/// FIXME: this needs to be configurable
pub const SOCKET_CTL_PATH: &str = "/tmp/vermilion.ctl";

/// Target for command, either a name or a pid
#[derive(Serialize, Deserialize, Debug)]
pub enum Target {
    Name(String),
    Pid(u32),
}

/// Commands the leader supports
#[derive(Serialize, Deserialize, Debug)]
pub enum Command {
    /// Initialize (and start) a process
    Init(Target),
    /// Start a process, if started, do nothing
    Start(Target),
    /// Restart a process, i.e. stop and start
    Restart(Target),
    /// Stop a process, if stopped, do nothing
    Stop(Target),
    /// Status of a specific process
    Status(Target),
    /// List all registerd processes and their statuses
    List,
}

impl msg::ToMessageKind for Command {
    fn to_message_kind(self) -> Result<(msg::MessageKind, Option<RawFd>), Error> {
        Ok((msg::MessageKind::Command(self), None))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CommandResponse {
    ListItem {
        name: String,
        id: u32,
        status: Status,
    },
}

impl msg::ToMessageKind for CommandResponse {
    fn to_message_kind(self) -> Result<(msg::MessageKind, Option<RawFd>), Error> {
        Ok((msg::MessageKind::CommandResponse(self), None))
    }
}

/// Issue commands to the Launcher
///
/// Rules:
///   - should be unprivileged
///   - stdin for control
///   - puts messages onto the IPC
#[derive(Debug)]
pub struct Leader;

#[async_trait]
impl Process<NoCtlIn, HasCtlOut> for Leader {
    const NAME: &'static str = "leader";

    fn inner_sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME).about("Controller for issuing instructions to Vermilion")
    }

    async fn run(self, args: &ArgMatches<'_>) -> Result<(), Error> {
        println!("Leader started");
        eprintln!("Leader seriously started");

        let mut control = Self::get_ctl_out(args)
            .expect("failed to get control-out")
            .expect("control-out parameter not present");

        let mut ctl_socket = UnixListener::bind(SOCKET_CTL_PATH)?;
        let mut incoming = ctl_socket.incoming();
        let (tx, rx) = channel::<Command>(3);

        tokio::spawn(handle_commands(control, rx));

        while let Some(stream) = incoming.next().await {
            match stream {
                Ok(stream) => {
                    tokio::spawn(handle_stream(stream, tx.clone()));
                }
                Err(e) => eprintln!("incomming connection failure: {}", e),
            }
        }

        Err(Error::from(
            "Leader unexpectedly exiting, no mor incoming streams",
        ))
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

async fn handle_commands(control: AsyncCtlEnd<Write>, mut rx: Receiver<Command>) {
    while let Some(command) = rx.recv().await {
        // msg::Metadata::new()
    }
}

async fn handle_stream(stream: UnixStream, tx: Sender<Command>) {}
