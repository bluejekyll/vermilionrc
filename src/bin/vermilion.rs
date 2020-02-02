// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::ffi::OsString;
use std::os::unix::io::FromRawFd;

use clap::{App, Arg, ArgMatches, SubCommand};
use futures::future::FutureExt;
use futures::select;
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStderr, ChildStdout};
use tokio::runtime;

use vermilionrc::control::{AsyncCtlEnd, CtlEnd};
use vermilionrc::fork::{new_process, Child};
use vermilionrc::msg::{Message, ToMessageKind};
use vermilionrc::pipe::{End, Pipe, PipeEnd, Read, Write};
use vermilionrc::procs::{self, CtlIn, CtlOut, Ipc, Launcher, Leader, Logger, Process, Supervisor};
use vermilionrc::Error;

trait SetupClapApp {
    fn setup_clap_app(self) -> Self;
}

impl<'a, 'b> SetupClapApp for App<'a, 'b> {
    fn setup_clap_app(self) -> Self {
        self.version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
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
        .subcommand(Leader::sub_command().setup_clap_app())
        .subcommand(Logger::sub_command().setup_clap_app())
        .subcommand(Launcher::sub_command().setup_clap_app())
        .subcommand(Ipc::sub_command().setup_clap_app())
        .subcommand(Supervisor::sub_command().setup_clap_app())
        .get_matches();

    let mut runtime = runtime::Builder::new()
        .basic_scheduler()
        .enable_io()
        .build()
        .expect("Failed to initialize Tokio Runtime");

    runtime.block_on(async move {
        match args.subcommand() {
            (procs::INIT, Some(args)) => init(args).await,
            (Leader::NAME, Some(args)) => run(Leader, args).await,
            (Logger::NAME, Some(args)) => run(Logger, args).await,
            (Launcher::NAME, Some(args)) => run(Launcher, args).await,
            (Ipc::NAME, Some(args)) => run(Ipc, args).await,
            (Supervisor::NAME, Some(args)) => run(Supervisor, args).await,
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
    // Start the logger first, as it will receive all stdouts and stderrs from other processes
    let mut logger = new_process(Logger).expect("failed to start logger");
    let mut logger_control = logger
        .take_control_in()
        .expect("no control")
        .into_async_ctl_end()?;

    // Now that we have the logger, we can start all the other processes and hand over their output handles
    //
    // Start the Leader, we need a control output to for the IPC from that to use
    let mut leader = new_process(Leader).expect("failed to start the leader");
    send_output_to_logger(&mut leader, &mut logger_control)
        .await
        .expect("failed to send output to logger for leader");

    // Start the Launcher which will give us the ability to spawn processes
    let mut launcher = new_process(Launcher).expect("failed to start the launcher");
    send_output_to_logger(&mut launcher, &mut logger_control)
        .await
        .expect("failed to send output to logger for launcher");

    // Ipc couldn't be started
    let mut ipc = new_process(Ipc).expect("failed to start the Ipc");
    send_output_to_logger(&mut ipc, &mut logger_control)
        .await
        .expect("failed to send output to logger for ipc");

    let pipe = Pipe::new().expect("failed to create pipe");
    let (reader, writer) = pipe.split();

    Message::<Read>::prepare_fd(reader)
        .send_msg(&mut logger_control)
        .await
        .expect("failed to send leader fd to logger");

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

    println!("vermilion started");

    // ensure that all processes continue to run
    let msg = select! {
        err = leader.wait().fuse() => format!("Leader unexpectedly exited: {:?}", err),
        err = logger.wait().fuse() => format!("Logger unexpectedly exited: {:?}", err),
        err = launcher.wait().fuse() => format!("Launcher unexpectedly exited: {:?}", err),
        err = ipc.wait().fuse() => format!("Ipc unexpectedly exited: {:?}", err),
        complete => format!("All Vermilion processes completed unexpectedly"),
    };

    Err(Error::from(format!("VermilionRC exited: {}", msg)))
}

async fn run<P: Process<I, O>, I: CtlIn, O: CtlOut>(
    _p: P,
    args: &ArgMatches<'_>,
) -> Result<(), Error> {
    println!("Running {}", P::NAME);
    P::run(args).await;

    Err(Error::from("Process unexpected ended"))
}

async fn send_output_to_logger(
    child: &mut Child,
    logger_ctl: &mut AsyncCtlEnd<Write>,
) -> Result<(), crate::Error> {
    let out: ChildStdout = child
        .take_stdout()
        .ok_or_else(|| Error::from("no stdout available"))?;
    Message::<Read>::prepare_fd(out)
        .send_msg(logger_ctl)
        .await?;

    let out: ChildStderr = child
        .take_stderr()
        .ok_or_else(|| Error::from("no stdout available"))?;

    Message::<Read>::prepare_fd(out).send_msg(logger_ctl).await
}
