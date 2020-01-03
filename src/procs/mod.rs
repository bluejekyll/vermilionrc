// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

mod ipc;
mod leader;
mod logger;
mod supervisor;

pub use ipc::ipc;
pub use leader::leader;
pub use logger::Logger;
pub use supervisor::supervisor;

use crate::control::CtlEnd;
use crate::fork::{FdAction, StdIo};
use crate::pipe::{PipeEnd, Read, Write};

pub trait Process: Send + 'static {
    fn run(self, control: CtlEnd<Read>);

    fn get_stdio() -> StdIo {
        StdIo {
            stdin: FdAction::Pipe,
            stderr: FdAction::Pipe,
            stdout: FdAction::Pipe,
        }
    }
}
