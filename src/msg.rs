// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::pin::Pin;
use std::task::Poll;

use bincode;
use futures::future::poll_fn;
use futures::ready;
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags};
use nix::sys::uio::IoVec;
use serde::{Deserialize, Serialize};

use crate::control::AsyncCtlEnd;
use crate::pipe::{End, PipeEnd, Read, Write};
use crate::Error;

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageKind {
    ReadPipeEnd,
    WritePipeEnd,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MessageData {
    kind: MessageKind,
    source: libc::pid_t,
}

/// This is not serializable, it's used to cary message metadata
pub struct Message<E: End> {
    data: MessageData,
    pipe: Option<PipeEnd<E>>,
}

impl Message<Read> {
    pub fn prepare_fd<P: Into<PipeEnd<Read>>>(pipe: P) -> Self {
        Self::prepare_fd_message(pipe.into(), MessageKind::ReadPipeEnd)
    }
}

impl Message<Write> {
    pub fn prepare_fd<P: Into<PipeEnd<Write>>>(pipe: P) -> Self {
        Self::prepare_fd_message(pipe.into(), MessageKind::WritePipeEnd)
    }
}

impl<E: End> Message<E> {
    fn prepare_fd_message(pipe: PipeEnd<E>, kind: MessageKind) -> Self {
        let pid = nix::unistd::Pid::this().as_raw();

        Self {
            data: MessageData { source: pid, kind },
            pipe: Some(pipe),
        }
    }

    pub fn take_pipe(&mut self) -> Option<PipeEnd<E>> {
        self.pipe.take()
    }

    pub async fn send_msg(mut self, target: &mut AsyncCtlEnd<Write>) -> Result<(), Error> {
        println!("sending message");
        let mut cmsg: [RawFd; 1] = [0; 1];
        let fd_to_send = match self.data.kind {
            MessageKind::ReadPipeEnd | MessageKind::WritePipeEnd => {
                let pipe = self
                    .pipe
                    .take()
                    .ok_or_else(|| Error::from("no pipe for Read or Write of Pipe"))?;

                println!("sending pipe: {:?}", pipe);

                // TODO: smallvec here
                cmsg[0] = pipe.into_raw_fd();
                &cmsg[..1]
            }
            _ => &cmsg[..0],
        };

        let mut target = Pin::new(target);
        let target = &mut target;

        let msg = bincode::serialize(&self.data)?;

        poll_fn(move |cx| {
            let control_message = ControlMessage::ScmRights(&fd_to_send);
            let msg_flags = MsgFlags::empty();
            let iov = IoVec::from_slice(&msg);
            let iov_to_send = &[iov];
            let control_message_to_send = &[control_message];
            target
                .as_mut()
                .poll_sendmsg(cx, iov_to_send, control_message_to_send, msg_flags, None)
        })
        .await?;
        Ok(())
    }

    pub async fn recv_msg(from: &mut AsyncCtlEnd<Read>) -> Result<Self, Error> {
        let mut from = Pin::new(from);
        let from = &mut from;
        let msg: Self = poll_fn(move |cx| {
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

            let message_data: MessageData = bincode::deserialize(recv_iovec_to_recv[0].as_slice())?;

            let pipe: Option<PipeEnd<E>> = match message_data.kind {
                MessageKind::ReadPipeEnd | MessageKind::WritePipeEnd => {
                    let cmsg = msg.cmsgs().next();

                    if let Some(ControlMessageOwned::ScmRights(raw_fd)) = cmsg {
                        // This is safe becuase the FD is being given to and only owned by the PipeEnd
                        unsafe {
                            Some(PipeEnd::from_raw_fd(
                                *raw_fd.first().ok_or_else(|| Error::from("no fd"))?,
                            ))
                        }
                    } else {
                        return Poll::Ready(Err(Error::from("Missing ScmRights from message")));
                    }
                }
                _ => None,
            };

            let msg = Message {
                data: message_data,
                pipe,
            };
            Poll::Ready(Ok(msg))
        })
        .await?;

        Ok(msg)
    }
}
