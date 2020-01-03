// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use nix::unistd::write;

use vermilionrc::control::Control;
use vermilionrc::fork::{new_process, StdIo, STDOUT};
use vermilionrc::msg;
use vermilionrc::pipe::Pipe;
use vermilionrc::procs::{ipc, leader, supervisor, Logger};

fn main() {
    // start the logger first
    let logger = new_process(Logger).expect("failed to start stdoutger");

    let pipe = Pipe::new().expect("failed to create pipe");

    let (reader, writer) = pipe.split();
    msg::send_read_fd(&logger.control, reader);

    write(writer.raw_fd(), "Vemilion say hello to logger".as_bytes()).expect("failed to write");

    // let (leader_read, leader_write) = pipe().expect("failed to create leader");
    // let (stdoutger_read, stdoutger_write) = pipe().expect("failed to create pipe for stdoutger");
    // let (ipc_read, ipc_write) = pipe().expect("failed to create pipe for ipc");
    // let (launcher_read, launcher_write) = pipe().expect("failed to create pipe for launcher");

    println!("vermilion started says hello");

    std::thread::sleep(std::time::Duration::from_millis(200));
}
