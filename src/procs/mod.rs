// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

mod ipc;
mod launcher;
mod leader;
mod logger;
mod supervisor;

pub use ipc::Ipc;
pub use launcher::Launcher;
pub use leader::Leader;
pub use logger::Logger;
pub use supervisor::Supervisor;

use std::ffi::OsString;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::process::Stdio;
use tokio::process::Command;

use async_trait::async_trait;
use clap::{App, Arg, ArgMatches};

use crate::control::{AsyncCtlEnd, CtlEnd};
use crate::fork::StdIoConf;
use crate::pipe::{End, Read, Write};
use crate::Error;

pub const CONTROL_IN: &str = "control-in";
pub const CONTROL_OUT: &str = "control-out";
pub const INIT: &str = "init";

/// A trait to define common construction of a process
///
/// ## Type Parameters
///
/// * D - Direction of Control, if the Leader this would be Write, all others would be Read
#[async_trait]
pub trait Process<I: CtlIn, O: CtlOut>: Sized + Send + 'static {
    const NAME: &'static str;

    fn inner_sub_command() -> App<'static, 'static>;

    fn sub_command() -> App<'static, 'static> {
        let app = Self::inner_sub_command();
        let app = app
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"));

        let app = Self::ctl_in_opts(app);
        Self::ctl_out_opts(app)
    }

    async fn run(self, args: &ArgMatches<'_>) -> Result<(), Error>;

    fn has_control_in() -> bool {
        I::has_control_in()
    }

    fn has_control_out() -> bool {
        O::has_control_out()
    }

    fn ctl_in_opts<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b> {
        I::ctl_in_opts(app)
    }

    fn ctl_out_opts<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b> {
        O::ctl_out_opts(app)
    }

    fn get_ctl_in(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Read>>, crate::Error> {
        I::get_ctl_in(args)
    }

    fn get_ctl_out(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Write>>, crate::Error> {
        O::get_ctl_out(args)
    }

    fn add_ctl_in_arg(child: &mut Command, ctl_in: &CtlEnd<Read>) {
        I::add_ctl_in_arg(child, ctl_in);
    }

    fn add_ctl_out_arg(child: &mut Command, ctl_out: &CtlEnd<Write>) {
        O::add_ctl_out_arg(child, ctl_out);
    }

    fn get_stdio() -> StdIoConf {
        StdIoConf {
            stdin: Stdio::piped(),
            stderr: Stdio::piped(),
            stdout: Stdio::piped(),
        }
    }
}

pub trait CtlIn: Sized + 'static {
    fn has_control_in() -> bool;
    fn ctl_in_opts<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b>;
    fn get_ctl_in(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Read>>, crate::Error>;
    fn add_ctl_in_arg(child: &mut Command, ctl_in: &CtlEnd<Read>);
}

pub trait CtlOut: Sized + 'static {
    fn has_control_out() -> bool;
    fn ctl_out_opts<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b>;
    fn get_ctl_out(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Write>>, crate::Error>;
    fn add_ctl_out_arg(child: &mut Command, ctl_out: &CtlEnd<Write>);
}

pub struct HasCtlIn;

impl CtlIn for HasCtlIn {
    fn ctl_in_opts<'a, 'b>(mut app: App<'a, 'b>) -> App<'a, 'b> {
        app.arg(
            Arg::with_name(CONTROL_IN)
                .short("i")
                .long(CONTROL_IN)
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

    fn has_control_in() -> bool {
        true
    }

    fn get_ctl_in(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Read>>, crate::Error> {
        let ctl_in = if let Some(ctl_in) = args.value_of_lossy(CONTROL_IN) {
            ctl_in
        } else {
            return Ok(None);
        };

        println!("opening ctl-in: {}", ctl_in);
        let fd = i32::from_str_radix(&ctl_in, 10)?;
        Ok(Some(unsafe {
            CtlEnd::<Read>::from_raw_fd(fd).into_async_ctl_end()?
        }))
    }

    fn add_ctl_in_arg(child: &mut Command, ctl_in: &CtlEnd<Read>) {
        child.arg(format!("--{}={}", CONTROL_IN, ctl_in.as_raw_fd()));
    }
}

pub struct NoCtlIn;

impl CtlIn for NoCtlIn {
    fn ctl_in_opts<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b> {
        app
    }

    fn has_control_in() -> bool {
        false
    }

    fn get_ctl_in(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Read>>, crate::Error> {
        Ok(None)
    }

    fn add_ctl_in_arg(_child: &mut Command, _ctl_in: &CtlEnd<Read>) {}
}

pub struct HasCtlOut;

impl CtlOut for HasCtlOut {
    fn ctl_out_opts<'a, 'b>(mut app: App<'a, 'b>) -> App<'a, 'b> {
        app.arg(
            Arg::with_name(CONTROL_OUT)
                .short("o")
                .long(CONTROL_OUT)
                .value_name("NUMBER")
                .validator_os(|i| {
                    i32::from_str_radix(&i.to_string_lossy(), 10)
                        .map(|_| ())
                        .map_err(|_| OsString::from("number was expected"))
                })
                .help("control output filedescriptor (used when forking the process)")
                .takes_value(true),
        )
    }

    fn has_control_out() -> bool {
        true
    }

    fn get_ctl_out(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Write>>, crate::Error> {
        let ctl_out = if let Some(ctl_out) = args.value_of_lossy(CONTROL_OUT) {
            ctl_out
        } else {
            return Ok(None);
        };

        println!("opening ctl-out: {}", ctl_out);
        let fd = i32::from_str_radix(&ctl_out, 10).expect("control-out is not a number");
        Ok(Some(unsafe {
            CtlEnd::<Write>::from_raw_fd(fd).into_async_ctl_end()?
        }))
    }

    fn add_ctl_out_arg(child: &mut Command, ctl_out: &CtlEnd<Write>) {
        child.arg(format!("--{}={}", CONTROL_OUT, ctl_out.as_raw_fd()));
    }
}

pub struct NoCtlOut;

impl CtlOut for NoCtlOut {
    fn ctl_out_opts<'a, 'b>(app: App<'a, 'b>) -> App<'a, 'b> {
        app
    }

    fn has_control_out() -> bool {
        false
    }

    fn get_ctl_out(args: &ArgMatches<'_>) -> Result<Option<AsyncCtlEnd<Write>>, crate::Error> {
        Ok(None)
    }

    fn add_ctl_out_arg(_child: &mut Command, _ctl_out: &CtlEnd<Write>) {}
}
