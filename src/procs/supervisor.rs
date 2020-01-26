// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::ffi::OsString;
use std::process::Stdio;

use async_trait::async_trait;
use clap::{App, Arg, ArgMatches, SubCommand};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::control::AsyncCtlEnd;
use crate::fork::StdIoConf;
use crate::msg::Message;
use crate::pipe::{AsyncPipeEnd, Read};
use crate::procs::Process;

static EXEC: &str = "executable";
static CMD_ARGS: &str = "cmd-args";
static MAX_STARTS: &str = "max-starts";

/// Launch and monitor processes
///
/// Rules:
///   - may listen to ipc pipe for commands
///   - should restart failed processes
///   - should send stdout from child prcess to IPC (and on to logger)
///   - stdin should be inherited?
pub struct Supervisor;

#[async_trait]
impl Process<Read> for Supervisor {
    const NAME: &'static str = "supervisor";

    fn sub_command() -> App<'static, 'static> {
        SubCommand::with_name(Self::NAME)
            .about("Process monitor for all Vermilion")
            .arg(
                Arg::with_name(EXEC)
                    .short("e")
                    .long(EXEC)
                    .value_name("COMMAND")
                    .help("path to the command to be run")
                    .required(true)
                    .takes_value(true),
            )
            .arg(
                Arg::with_name(MAX_STARTS)
                    .short("m")
                    .long(MAX_STARTS)
                    .value_name("NUMBER")
                    .help("maximum number of starts before exiting")
                    .takes_value(true)
                    .default_value("1")
                    .validator_os(|i| {
                        u8::from_str_radix(&i.to_string_lossy(), 10)
                            .map(|_| ())
                            .map_err(|_| OsString::from("number was expected"))
                    }),
            )
            .arg(
                Arg::with_name(CMD_ARGS)
                    .raw(true)
                    .help("arguments to pass to the new process"),
            )
    }

    async fn run(control: AsyncCtlEnd<Read>, args: &ArgMatches<'_>) {
        println!("Supervisor started");

        // How many times should the process be restarted?
        let max_restarts = u8::from_str_radix(
            &args
                .value_of_lossy(MAX_STARTS)
                .expect("there should have been a default value"),
            10,
        )
        .expect("bad value for max_restarts");

        // get the executable
        let executable = args.value_of_os(EXEC).expect("executable not specified");
        println!("Starting {:?}", executable);

        // Restart up to the max_restart value
        for _ in 0..max_restarts {
            let mut cmd = Command::new(executable);

            cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            //.uid(uid)
            //.gid(gid)
            ;

            // add all arguments
            if let Some(cmd_args) = args.values_of_lossy(CMD_ARGS) {
                cmd.args(cmd_args);
            }

            cmd.spawn()
                .expect("failed to start process")
                .await
                .expect("process failed to run");
        }
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
