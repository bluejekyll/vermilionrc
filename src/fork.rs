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

use nix::unistd::dup2;
use tokio::process::ChildStdout;
use tokio::process::Command;

use crate::control::{Control, CtlEnd};
use crate::pipe::{End, Write};
use crate::procs::{self, FlipControl, Process};
use crate::Error;

pub(crate) const NULL: RawFd = -1;
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
pub struct Child<E: End> {
    child: tokio::process::Child,
    control: Option<CtlEnd<E>>,
}

impl<E: End> Child<E> {
    pub fn stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_control(&mut self) -> Option<CtlEnd<E>> {
        self.control.take()
    }

    pub async fn wait(self) -> Result<ExitStatus, Error> {
        Ok(self.child.await?)
    }
}

pub fn new_process<P, E>(_child_task: P) -> Result<Child<E::Reverse>, &'static str>
where
    P: Process<E>,
    E: End + FlipControl<E>,
{
    let stdio = P::get_stdio();

    let control = Control::new().expect("failed to create control sockets");
    let (read, write) = control.split();
    let (ctl_input, ctl_output) = E::flip(read, write);

    // FIXME: clear env? set working directory? uid/gid?
    let child = Command::new(std::env::args_os().next().expect("arg0 is not present?"))
        .arg(P::NAME)
        .arg(format!("--{}={}", procs::CONTROL_IN, ctl_input.as_raw_fd()))
        .kill_on_drop(true)
        .stdin(stdio.stdin)
        .stdout(stdio.stdout)
        .stderr(stdio.stderr)
        .spawn()
        .map_err(|_| "failed to spawn process")?;

    let control = ctl_output;
    println!("started child process control: {:?}", control);

    Ok(Child {
        child,
        control: Some(control),
    })
}
