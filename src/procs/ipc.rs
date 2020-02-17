// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::collections::HashMap;
use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, ArgMatches, SubCommand};
use tokio::io::AsyncReadExt;
use tokio::select;

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg::{Message, Metadata};
use crate::pipe::{Read, Write};
use crate::procs::leader::{self, Command, CommandResponse};
use crate::procs::{self, CtlIn, CtlOut, HasCtlIn, HasCtlOut, Process};
use crate::Error;

/// All message types expected by Ipc
enum IpcMessage {
    /// Messages recieved from the Leader
    FromLeader(Message),
}

/// Inter-process-communucation service.
///
/// Recv messages from the Leader and pass to supervisors...
///
/// Rules:
///  - may only receive message from the leader.
///  - may only deliver messages to the supervisors and launcher
///    - must validate message from leader
///    - never deliver messages to the leader except from launcer, pid's etc.
#[derive(Debug)]
pub struct Ipc;

#[async_trait]
impl Process<HasCtlIn, HasCtlOut> for Ipc {
    const NAME: &'static str = "ipc";

    /// CLI SubCommand arguments
    fn inner_sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME)
            .about("Inter-process communication for Vermilion processes")
    }

    /// This should be the ctl in port from the leader
    async fn run(self, args: &ArgMatches<'_>) -> Result<(), Error> {
        println!("Ipc started");

        let mut control =
            HasCtlIn::get_ctl_in(args)?.ok_or_else(|| "control-in is a required parameter")?;

        // all of these are passed over...
        let logger: Logger = get_logger(&mut control).await?;
        let mut leader: Leader = get_leader(&mut control).await?;
        let launcher: Launcher = get_launcher(&mut control).await?;

        // processes
        let proc_names: HashMap<String, libc::pid_t> = HashMap::new();
        let procs: HashMap<libc::pid_t, Supervised> = HashMap::new();

        for i in 0..60 {
            #[allow(clippy::modulo_one)]
            let message: Result<IpcMessage, Error> = select! {
               // listen to the leader for messages to the launcher or target processes...
               //    send message
               //    how to handle Read requests, like list processes
               //      requesting process sends a ctl through the Leader,
               //      and that is used for the response...
               message = leader.recv_msg() => message.map(IpcMessage::FromLeader),
               else => unimplemented!(),
            };

            match message {
                Ok(IpcMessage::FromLeader(message)) => {
                    let (cmd, ctl_out) = handle_leader_message(message, &mut leader).await?;
                    match cmd {
                        Command::Init(_) => unimplemented!(),
                        Command::Start(_) => unimplemented!(),
                        Command::Stop(_) => unimplemented!(),
                        Command::Restart(_) => unimplemented!(),
                        Command::Status(_) => unimplemented!(),
                        Command::List => {
                            for supervised in &procs {
                                // TODO: ask for results from each process
                            }
                            // TODO: send end of list...
                        }
                    }
                }
                Err(e) => {
                    println!("error recieving message: {}", e);
                }
            }

            // let fd = message.take_fd().ok_or_else(|| "expected an fd")?;
            // println!("received filedescriptor: {:?}", fd);

            // let mut reader = fd.into;

            // let mut buf = [0u8; 1024];
            // let len = reader.read(&mut buf).await.expect("failed to read");
            // let line = String::from_utf8_lossy(&buf[..len]);
            // println!("LOG_LINE: {}", line);

            // listen to our control in for new processes from the Launcher
            //    send all pipes to logger
            //    register ctl's to procs

            //     println!("{} awaiting input: {}", Ipc::NAME, i);
            //     std::thread::sleep(std::time::Duration::from_secs(1))
        }

        unimplemented!()
    }

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            // we need a new input line
            stdin: Stdio::piped(),
            // StdErr and stdout will be piped to the logger
            stderr: Stdio::piped(),
            stdout: Stdio::piped(),
        }
    }
}

struct Logger(libc::pid_t, AsyncCtlEnd<Write>);
struct Leader(libc::pid_t, AsyncCtlEnd<Read>);

impl Leader {
    async fn recv_msg(&mut self) -> Result<Message, Error> {
        Message::recv_msg(&mut self.1).await
    }
}

struct Launcher(libc::pid_t, AsyncCtlEnd<Write>);

/// Ensure the source pid is the original parent, i.e. VermilionRC/Launcher
fn verify_pid(_msg: &Metadata) -> Result<(), Error> {
    // FIXME: check that the ppid matches the src on the message
    Ok(())
}

async fn get_logger(control: &mut AsyncCtlEnd<Read>) -> Result<Logger, Error> {
    let mut message = Message::recv_msg(control).await?;

    let pid = match message
        .metadata()
        .proc_name()
        .expect("name required for registration")
    {
        procs::Logger::NAME => message
            .metadata()
            .pid()
            .expect("pid required with logger registration"),
        name => return Err(format!("Wrong process: {}", name).into()),
    };

    let ctl = message
        .take_write_ctl_end()
        .ok_or_else(|| "expected write ctl end")?;
    Ok(Logger(pid, ctl))
}

async fn get_leader(control: &mut AsyncCtlEnd<Read>) -> Result<Leader, Error> {
    let mut message = Message::recv_msg(control).await?;

    let pid = match message
        .metadata()
        .proc_name()
        .expect("name required for registration")
    {
        procs::Logger::NAME => message
            .metadata()
            .pid()
            .expect("pid required for registration"),
        name => return Err(format!("Wrong process: {}", name).into()),
    };

    let ctl = message
        .take_read_ctl_end()
        .ok_or_else(|| "expected read ctl end")?;
    Ok(Leader(pid, ctl))
}

async fn get_launcher(control: &mut AsyncCtlEnd<Read>) -> Result<Launcher, Error> {
    let mut message = Message::recv_msg(control).await?;

    let pid = match message
        .metadata()
        .proc_name()
        .expect("name required for registration")
    {
        procs::Logger::NAME => message
            .metadata()
            .pid()
            .expect("pid required for registration"),
        name => return Err(format!("Wrong process: {}", name).into()),
    };

    let ctl = message
        .take_write_ctl_end()
        .ok_or_else(|| "expected write ctl end")?;
    Ok(Launcher(pid, ctl))
}

/// This takes a message from the Leader
///
/// # Returns
///
/// The issued command and the response channel for all results.
async fn handle_leader_message(
    mut message: Message,
    leader: &mut Leader,
) -> Result<(leader::Command, AsyncCtlEnd<Write>), Error> {
    // get the command being issued
    let cmd = message
        .take_kind()
        .ok_or_else(|| Error::from("Something stole our command"))?
        .into_command()
        .map_err(|_| Error::from("expected a command from the Leader"))?;

    // Leader should always send another message along witht eh output channel for the result
    //   this prevent the potential that leader could recieve commands from an unprivileged sub-process
    let mut message = leader.recv_msg().await?;

    // FIXME: verify message metadata
    // TODO: let's just combine this into the initial message
    let ctl_out = message
        .take_write_ctl_end()
        .ok_or_else(|| "expected write ctl end")?;

    Ok((cmd, ctl_out))
}

struct Supervised {
    id: libc::pid_t,
    name: String,
    supervisor_ctl: AsyncCtlEnd<Write>,
}
