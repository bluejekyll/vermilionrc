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

// pub enum FdAction {
//     // Uses the existing file descriptors, i.e. STDIN for stdin
//     Inherit,
//     // Create a new set of file descriptors for the pipe
//     Pipe,
//     // do no create a pipe, or inherit the file handle
//     Null,
// }

// impl FdAction {
//     fn new_pipe_or_existing(
//         &self,
//         existing_read: RawFd,
//         existing_write: RawFd,
//     ) -> nix::Result<Pipe> {
//         // This is safe, as all the fd are either singly owned or understood to be shared
//         unsafe {
//             match self {
//                 FdAction::Inherit => Ok(Pipe::from_raw_fd(existing_read, existing_write)),
//                 FdAction::Pipe => Pipe::new(),
//                 FdAction::Null => Ok(Pipe::from_raw_fd(NULL, NULL)),
//             }
//         }
//     }
// }

pub struct StdIoConf {
    pub stdin: Stdio,
    pub stderr: Stdio,
    pub stdout: Stdio,
}

// impl StdIoConf {
//     pub fn new_stdin(&self) -> nix::Result<Pipe> {
//         self.stdin.new_pipe_or_existing(STDIN, NULL)
//     }

//     pub fn new_stderr(&self) -> nix::Result<Pipe> {
//         self.stderr.new_pipe_or_existing(NULL, STDERR)
//     }

//     pub fn new_stdout(&self) -> nix::Result<Pipe> {
//         self.stdout.new_pipe_or_existing(NULL, STDOUT)
//     }
// }

pub struct Child {
    pub child: tokio::process::Child,
    pub control: CtlEnd<Write>,
}

pub fn new_process<P>(child_task: P) -> Result<Child, &'static str>
where
    P: Process,
{
    let stdio = P::get_stdio();

    // let stdin = stdio.new_stdin()?;
    // let stderr = stdio.new_stderr()?;
    // let stdout = stdio.new_stdout()?;

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

    // let fork_result = fork()?;

    // // This was the parent,
    // if let ForkResult::Parent { child } = fork_result {
    //     let child = Child {
    //         pid: child,
    //         stdin: stdin.take_writer(),
    //         stderr: stderr.take_reader(),
    //         stdout: stdout.take_reader(),
    //         control: control.take_writer(),
    //     };

    //     return Ok(child);
    // }

    // // clean up, close all unwanted filehandles
    // let stdin = stdin.take_reader();
    // let stderr = stderr.take_writer();
    // let stdout = stdout.take_writer();

    let control = write;
    println!("started child process control: {:?}", control);

    Ok(Child { child, control })

    // swap stdin, stdout and stderr (goes to stdout)
    //stdin.replace(STDIN)?;
    //stderr.replace(STDERR)?;
    //stdout.replace(STDOUT)?;

    //    child_task.run(control);

    //  std::thread::sleep(std::time::Duration::from_millis(100));

    //Err(nix::Error::UnsupportedOperation)
    //panic!("child task ended");
}
