// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::process::ExitStatus;
use std::process::Stdio;

use tokio::process::Command;
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};

use crate::control::{Control, CtlEnd};
use crate::pipe::{End, Read, Write};
use crate::procs::{self, CtlIn, CtlOut, Process};
use crate::Error;

pub const STDIN: RawFd = 0; // Control In
pub const STDOUT: RawFd = 1; // stdout Out
pub const STDERR: RawFd = 2; // Control Out

pub struct StdIoConf {
    pub stdin: Stdio,
    pub stderr: Stdio,
    pub stdout: Stdio,
}

/// The Child process.
///
/// # Type parameters
///
/// * E - Direction of the control endpoint, Write means it takes input from this channel (most), Read means it issues output (generally only the Leader)
pub struct Child {
    child: tokio::process::Child,
    control_in: Option<CtlEnd<Write>>,
    control_out: Option<CtlEnd<Read>>,
}

impl Child {
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_control_in(&mut self) -> Option<CtlEnd<Write>> {
        self.control_in.take()
    }

    pub fn take_control_out(&mut self) -> Option<CtlEnd<Read>> {
        self.control_out.take()
    }

    pub async fn wait(self) -> Result<ExitStatus, Error> {
        Ok(self.child.await?)
    }
}

pub fn new_process<P, I, O>(_child_task: P) -> Result<Child, &'static str>
where
    P: Process<I, O>,
    I: CtlIn,
    O: CtlOut,
{
    let stdio = P::get_stdio();

    let mut child = Command::new(std::env::args_os().next().expect("arg0 is not present?"));
    child
        .arg(P::NAME)
        .kill_on_drop(true)
        .stdin(stdio.stdin)
        .stdout(stdio.stdout)
        .stderr(stdio.stderr);

    let mut control_in = (None, None);
    if P::has_control_in() {
        let control = Control::new().expect("failed to create control sockets");
        let (read, write) = control.split();

        P::add_ctl_in_arg(&mut child, &read);
        control_in = (Some(read), Some(write));
    }

    let mut control_out = (None, None);
    if P::has_control_out() {
        let control = Control::new().expect("failed to create control sockets");
        let (read, write) = control.split();

        P::add_ctl_out_arg(&mut child, &write);
        control_out = (Some(read), Some(write));
    }

    // FIXME: clear env? set working directory? uid/gid?
    let child = child.spawn().map_err(|_| "failed to spawn process")?;

    println!(
        "Started {} process ({}) control_in: {:?}",
        P::NAME,
        child.id(),
        control_in.1,
    );

    println!(
        "Started {} process ({}) control_out: {:?}",
        P::NAME,
        child.id(),
        control_out.0,
    );

    // drop, i.e. close, the FDs that the child process used now, they must live until after the prcess is forked
    control_in.0.take();
    control_out.1.take();

    Ok(Child {
        child,
        control_in: control_in.1.take(),
        control_out: control_out.0.take(),
    })
}
