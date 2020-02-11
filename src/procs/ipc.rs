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

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg::{Message, Metadata};
use crate::pipe::{Read, Write};
use crate::procs::{self, CtlIn, CtlOut, HasCtlIn, HasCtlOut, Process};
use crate::Error;

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
        let leader: Leader = get_leader(&mut control).await?;
        let launcher: Launcher = get_launcher(&mut control).await?;
        let procs: HashMap<libc::pid_t, AsyncCtlEnd<Write>> = HashMap::new();

        // listen to our control in for new processes from the Launcher
        //    send all pipes to logger
        //    register ctl's to procs

        // listen to the leader for messages to the launcher or target processes...
        //    send message
        //    how to handle Read requests, like list processes
        //      requesting process sends a ctl through the Leader,
        //      and that is used for the response...
        let mut message = Message::recv_msg(&mut control)
            .await
            .expect("no msg received");

        // let fd = message.take_fd().ok_or_else(|| "expected an fd")?;
        // println!("received filedescriptor: {:?}", fd);

        // let mut reader = fd.into;

        // let mut buf = [0u8; 1024];
        // let len = reader.read(&mut buf).await.expect("failed to read");
        // let line = String::from_utf8_lossy(&buf[..len]);
        // println!("LOG_LINE: {}", line);

        // for i in 0..60 {
        //     println!("{} awaiting input: {}", Ipc::NAME, i);
        //     std::thread::sleep(std::time::Duration::from_secs(1))
        // }

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
struct Launcher(libc::pid_t, AsyncCtlEnd<Write>);

/// Ensure the source pid is the original parent, i.e. VermilionRC/Launcher
fn verify_pid(_msg: &Metadata) -> Result<(), Error> {
    // FIXME: check that the ppid matches the src on the message
    Ok(())
}

async fn get_logger(control: &mut AsyncCtlEnd<Read>) -> Result<Logger, Error> {
    let mut message = Message::recv_msg(control).await?;

    let pid = match message.metadata().proc_name() {
        procs::Logger::NAME => message.metadata().pid(),
        name => return Err(format!("Wrong process: {}", name).into()),
    };

    let ctl = message
        .take_write_ctl_end()
        .ok_or_else(|| "expected write ctl end")?;
    Ok(Logger(pid, ctl))
}

async fn get_leader(control: &mut AsyncCtlEnd<Read>) -> Result<Leader, Error> {
    let mut message = Message::recv_msg(control).await?;

    let pid = match message.metadata().proc_name() {
        procs::Logger::NAME => message.metadata().pid(),
        name => return Err(format!("Wrong process: {}", name).into()),
    };

    let ctl = message
        .take_read_ctl_end()
        .ok_or_else(|| "expected read ctl end")?;
    Ok(Leader(pid, ctl))
}

async fn get_launcher(control: &mut AsyncCtlEnd<Read>) -> Result<Launcher, Error> {
    let mut message = Message::recv_msg(control).await?;

    let pid = match message.metadata().proc_name() {
        procs::Logger::NAME => message.metadata().pid(),
        name => return Err(format!("Wrong process: {}", name).into()),
    };

    let ctl = message
        .take_write_ctl_end()
        .ok_or_else(|| "expected write ctl end")?;
    Ok(Launcher(pid, ctl))
}
