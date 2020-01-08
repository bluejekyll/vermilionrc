// Copyright 2019-2020 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

/// Launch and monitor processes
///
/// Rules:
///   - may listen to ipc pipe for commands
///   - should restart failed processes
///   - should send stdout from child prcess to IPC (and on to logger)
///   - stdin should be inherited?
pub fn supervisor() {}
