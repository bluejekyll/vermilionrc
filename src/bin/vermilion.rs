// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::ffi::OsString;
use std::os::unix::io::FromRawFd;

use clap::{App, Arg, ArgMatches, SubCommand};
use futures::select;
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdout;
use tokio::runtime;

use vermilionrc::control::CtlEnd;
use vermilionrc::fork::new_process;
use vermilionrc::msg;
use vermilionrc::pipe::{End, Pipe, Read, Write};
use vermilionrc::procs::{self, Leader, Logger, Process};
use vermilionrc::Error;

trait SetupClapApp {
    fn setup_clap_app(self) -> Self;
    fn default_subcommand_opts(self) -> Self;
}

impl<'a, 'b> SetupClapApp for App<'a, 'b> {
    fn setup_clap_app(self) -> Self {
        self.version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
    }

    fn default_subcommand_opts(self) -> Self {
        self.arg(
            Arg::with_name(procs::CONTROL_IN)
                .short("c")
                .long(procs::CONTROL_IN)
                .value_name("NUMBER")
                .validator_os(|i| {
                    i32::from_str_radix(&i.to_string_lossy(), 10)
                        .map(|_| ())
                        .map_err(|_| OsString::from("number was expected"))
                })
                .help("control input filedescriptor (used when forking the process)")
                .takes_value(true),
        )
    }
}

fn init_sub_command() -> App<'static, 'static> {
    SubCommand::with_name(procs::INIT).about("initialize the VermilionRC framework")
}

fn main() -> Result<(), Error> {
    let args = App::new(env!("CARGO_PKG_NAME"))
        .setup_clap_app()
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .subcommand(init_sub_command().setup_clap_app())
        .subcommand(
            Logger::sub_command()
                .setup_clap_app()
                .default_subcommand_opts(),
        )
        .subcommand(
            Leader::sub_command()
                .setup_clap_app()
                .default_subcommand_opts(),
        )
        .get_matches();

    let mut runtime = runtime::Builder::new()
        .basic_scheduler()
        .enable_io()
        .build()
        .expect("Failed to initialize Tokio Runtime");

    runtime.block_on(async move {
        match args.subcommand() {
            (procs::INIT, Some(args)) => init(args).await,
            (Logger::NAME, Some(args)) => run::<Logger, Read>(args).await,
            (Leader::NAME, Some(args)) => run::<Leader, Write>(args).await,
            ("", None) => {
                println!("command required");
                println!("{}", args.usage());
                std::process::exit(1);
            }
            (arg, _) => {
                println!("unexpected argument: {}", arg);
                println!("{}", args.usage());
                std::process::exit(2);
            }
        }
    })
}

async fn init(_args: &ArgMatches<'_>) -> Result<(), Error> {
    // Start the logger first
    let mut logger = new_process(Logger).expect("failed to start logger");
    let mut logger_control = logger
        .take_control()
        .expect("no control")
        .into_async_pipe_end()?;

    // Now that we have the logger, we can start all the other processes and hand over their output handles
    //
    // Start the Leader, we need a control output to for the IPC from that to use
    let mut leader = new_process(Leader).expect("failed to start the leader");
    let stdout: ChildStdout = leader.stdout().expect("no stdout from leader");

    msg::send_fd(&mut logger_control, stdout)
        .await
        .expect("failed to send leader fd to logger");

    let pipe = Pipe::new().expect("failed to create pipe");

    let (reader, writer) = pipe.split();
    msg::send_fd(&mut logger_control, reader)
        .await
        .expect("failed to send filedescriptor");

    let mut writer = writer
        .into_async_pipe_end()
        .expect("failed to get UnixStream");
    writer
        .write_all(b"Vemilion says hello to logger\n")
        .await
        .expect("failed to write");
    writer
        .write_all(b"You're cool logger\n")
        .await
        .expect("failed to write");
    drop(writer);

    // let (leader_read, leader_write) = pipe().expect("failed to create leader");
    // let (logger_read, logger_write) = pipe().expect("failed to create pipe for logger");
    // let (ipc_read, ipc_write) = pipe().expect("failed to create pipe for ipc");
    // let (launcher_read, launcher_write) = pipe().expect("failed to create pipe for launcher");

    println!("vermilion started");

    println!(
        "leader exited: {}",
        leader.wait().await.expect("logger failed")
    );
    println!(
        "logger exited: {}",
        logger.wait().await.expect("logger failed")
    );

    Err(Error::from("VermilionRC exited"))
}

async fn run<P: Process<E>, E: End>(args: &ArgMatches<'_>) -> Result<(), Error> {
    let ctl_in = args
        .value_of_lossy(procs::CONTROL_IN)
        .expect("control-in not specified");

    let fd = i32::from_str_radix(&ctl_in, 10).expect("control-in is not a number");
    let ctl = unsafe { CtlEnd::<E>::from_raw_fd(fd) };

    println!("Running {}", P::NAME);
    P::run(ctl.into_async_pipe_end()?).await;

    Err(Error::from("Process unexpected ended"))
}
