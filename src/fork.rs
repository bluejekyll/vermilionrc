// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::process::Stdio;

use nix::unistd::{close, fork, ForkResult, Pid};
use tokio::process::Command;

use crate::control::{Control, CtlEnd};
use crate::pipe::{Pipe, PipeEnd, Read, Write};
use crate::procs::{self, Process};

pub const NULL: RawFd = -1;
pub const STDIN: RawFd = 0; // Control In
pub const STDOUT: RawFd = 1; // stdout Out
pub const STDERR: RawFd = 2; // Control Out

pub struct StdIoConf {
    pub stdin: Stdio,
    pub stderr: Stdio,
    pub stdout: Stdio,
}

pub struct Child {
    pub child: tokio::process::Child,
    pub control: CtlEnd<Write>,
}

pub fn new_process<P>(child_task: P) -> Result<Child, &'static str>
where
    P: Process,
{
    let stdio = P::get_stdio();

    let control = Control::new().expect("failed to create control sockets");
    let (read, write) = control.split();

    // FIXME: clear env? set working directory? uid/gid?
    let child = Command::new(std::env::args_os().next().expect("arg0 is not present?"))
        .arg(P::NAME)
        .arg(format!("--{}={}", procs::CONTROL_IN, read.as_raw_fd()))
        .kill_on_drop(true)
        .stdin(stdio.stdin)
        .stdout(stdio.stdout)
        .stderr(stdio.stderr)
        .spawn()
        .map_err(|_| "failed to spawn process")?;

    let control = write;
    println!("started child process control: {:?}", control);

    Ok(Child { child, control })
}
