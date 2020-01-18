// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::pin::Pin;
use std::task::Poll;

use futures::future::poll_fn;
use futures::ready;
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags};
use nix::sys::uio::IoVec;

use crate::control::AsyncCtlEnd;
use crate::pipe::{End, PipeEnd, Read, Write};
use crate::Error;

enum MessageType {
    ReadPipeEnd = 0_isize,
    WritePipeEnd,
}

pub async fn send_fd<R: AsRawFd>(
    target: &mut AsyncCtlEnd<Write>,
    fd_to_send_1: R,
) -> Result<(), Error> {
    let fd_to_send = &[fd_to_send_1.as_raw_fd()];
    std::mem::forget(fd_to_send_1);

    let mut target = Pin::new(target);
    let target = &mut target;

    poll_fn(move |cx| {
        let control_message = ControlMessage::ScmRights(fd_to_send);

        let kind = MessageType::WritePipeEnd as isize;
        let msg_flags = MsgFlags::empty();

        let kind_bytes = kind.to_ne_bytes();
        let iov = IoVec::from_slice(&kind_bytes[..]);

        let iov_to_send = &[iov];
        let control_message_to_send = &[control_message];
        target
            .as_mut()
            .poll_sendmsg(cx, iov_to_send, control_message_to_send, msg_flags, None)
    })
    .await?;

    Ok(())
}

pub async fn recv_msg<E: End>(from: &mut AsyncCtlEnd<Read>) -> Result<PipeEnd<E>, Error> {
    let mut from = Pin::new(from);
    let from = &mut from;

    let pipe = poll_fn(move |cx| {
        let mut recv_buf = [0u8; 1024];
        let recv_iovec = IoVec::from_mut_slice(&mut recv_buf);
        let recv_iovec_to_recv = [recv_iovec];
        let mut cmsg_buffer = Vec::with_capacity(32);
        let cmsg_ref: &mut Vec<_> = &mut cmsg_buffer;

        let msg = ready!(from.as_mut().poll_recvmsg(
            cx,
            &recv_iovec_to_recv,
            Some(cmsg_ref),
            MsgFlags::empty()
        ))?;

        println!("recieved: {}", msg.bytes);
        let cmsg = msg.cmsgs().next().expect("no cmsg");

        if let ControlMessageOwned::ScmRights(raw_fd) = cmsg {
            // This is safe becuase the FD is being given to and only owned by the PipeEnd
            unsafe { Poll::Ready(Ok(PipeEnd::from_raw_fd(*raw_fd.first().expect("no fd")))) }
        } else {
            Poll::Ready(Err(Error::from(nix::Error::UnsupportedOperation)))
        }
    })
    .await?;

    Ok(pipe)
}
