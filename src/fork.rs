// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use tokio::process::Command;

pub struct ForkParams {
    /// Set of environment variables it is safe to inherit from the parent
    inherit_parent_envs: Vec<std::ffi::OsString>,
    /// Working directory for the new process
    path: std::path::PathBuf,
    /// User Id to run as
    uid: u32,
    /// Group Id to run as
    gid: u32,
}

/// Cleans a command of any potentially leaked information to the child process
pub fn clean_command(mut command: Command, fork_params: ForkParams) -> Command {
    command.env_clear();

    // Fill in white_listed vairables
    for (key, var) in std::env::vars_os() {
        if fork_params.inherit_parent_envs.contains(&key) {
            command.env(key, var);
        }
    }

    // set the working directory to something explicitly
    command.current_dir(fork_params.path);

    // Kill the process if this reference is dropped
    command.kill_on_drop(true);

    command.uid(fork_params.uid);
    command.gid(fork_params.gid);

    command
}
