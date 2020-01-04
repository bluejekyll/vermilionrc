// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};

use nix::sys::socket::{recvmsg, sendmsg, ControlMessage, ControlMessageOwned, MsgFlags};
use nix::sys::uio::IoVec;

use crate::control::CtlEnd;
use crate::pipe::{End, PipeEnd, Read, Write};

enum MessageType {
    ReadPipeEnd = 0_isize,
    WritePipeEnd,
}

pub fn send_write_fd(target: &CtlEnd<Write>, fd_to_send: PipeEnd<Write>) -> nix::Result<()> {
    let fd_to_send = &[fd_to_send.into_raw_fd()];
    let control_message = ControlMessage::ScmRights(fd_to_send);

    let kind = MessageType::WritePipeEnd as isize;
    let msg_flags = MsgFlags::empty();

    let kind_bytes = kind.to_ne_bytes();
    let iov = IoVec::from_slice(&kind_bytes[..]);

    let iov_to_send = &[iov];
    let control_message_to_send = &[control_message];
    sendmsg(
        target.as_raw_fd(),
        iov_to_send,
        control_message_to_send,
        msg_flags,
        None,
    )?;

    Ok(())
}

pub fn send_read_fd(target: &CtlEnd<Write>, fd_to_send: PipeEnd<Read>) -> nix::Result<()> {
    let fd_to_send = &[fd_to_send.into_raw_fd()];
    let control_message = ControlMessage::ScmRights(fd_to_send);

    let kind = MessageType::ReadPipeEnd as isize;
    let msg_flags = MsgFlags::empty();

    let kind_bytes = kind.to_ne_bytes();
    let iov = IoVec::from_slice(&kind_bytes[..]);

    let iov_to_send = &[iov];
    let control_message_to_send = &[control_message];
    sendmsg(
        target.as_raw_fd(),
        iov_to_send,
        control_message_to_send,
        msg_flags,
        None,
    )?;

    Ok(())
}

pub fn recv_msg(from: &CtlEnd<Read>) -> nix::Result<PipeEnd<Read>> {
    let mut recv_buf = [0u8; 1024];
    let mut recv_iovec = IoVec::from_mut_slice(&mut recv_buf);
    let recv_iovec_to_recv = [recv_iovec];
    let mut cmsg_buffer = Vec::with_capacity(32);

    let msg = recvmsg(
        from.as_raw_fd(),
        &recv_iovec_to_recv,
        Some(&mut cmsg_buffer),
        MsgFlags::empty(),
    )?;

    println!("recieved: {}", msg.bytes);
    let cmsg = msg.cmsgs().next().expect("no cmsg");

    if let ControlMessageOwned::ScmRights(raw_fd) = cmsg {
        // This is safe becuase the FD is being given to and only owned by the PipeEnd
        unsafe { Ok(PipeEnd::from_raw_fd(*raw_fd.first().expect("no fd"))) }
    } else {
        Err(nix::Error::UnsupportedOperation)
    }
}
