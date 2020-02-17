// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::pin::Pin;
use std::task::Poll;

use bincode;
use enum_as_inner::EnumAsInner;
use futures::future::poll_fn;
use futures::ready;
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags};
use nix::sys::uio::IoVec;
use serde::{Deserialize, Serialize};
use tokio::process::{ChildStderr, ChildStdout};

use crate::control::{AsyncCtlEnd, CtlEnd};
use crate::pipe::{AsyncPipeEnd, End, PipeEnd, Read, Write};
use crate::procs::leader;
use crate::Error;

#[derive(Serialize, Deserialize, Debug, EnumAsInner)]
pub enum MessageKind {
    ReadPipeEnd,
    WritePipeEnd,
    ReadCtlEnd,
    WriteCtlEnd,
    Command(leader::Command),
    CommandResponse(leader::CommandResponse),
}

pub trait ToMessageKind {
    /// Convert self into the MessageData to send and an optional filehandle
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error>;
}

impl ToMessageKind for PipeEnd<Read> {
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error> {
        Ok((MessageKind::ReadPipeEnd, Some(self.into_raw_fd())))
    }
}

impl ToMessageKind for PipeEnd<Write> {
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error> {
        Ok((MessageKind::WritePipeEnd, Some(self.into_raw_fd())))
    }
}

impl ToMessageKind for &ChildStdout {
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error> {
        Ok((MessageKind::ReadPipeEnd, Some(self.as_raw_fd())))
    }
}

impl ToMessageKind for &ChildStderr {
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error> {
        Ok((MessageKind::ReadPipeEnd, Some(self.as_raw_fd())))
    }
}

impl ToMessageKind for AsyncCtlEnd<Read> {
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error> {
        Ok((MessageKind::ReadCtlEnd, Some(self.into_raw_fd()?)))
    }
}

impl ToMessageKind for AsyncCtlEnd<Write> {
    fn to_message_kind(self) -> Result<(MessageKind, Option<RawFd>), Error> {
        Ok((MessageKind::WriteCtlEnd, Some(self.into_raw_fd()?)))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Metadata {
    /// The name of the process this is associated to
    proc_name: Option<String>,
    /// The process id of the associated handle
    pid: Option<libc::pid_t>,
    /// The process that sent the message
    source_pid: libc::pid_t,
    /// The process for which this is targeted
    target_pid: Option<libc::pid_t>,
}

impl Metadata {
    pub fn new(
        proc_name: Option<String>,
        pid: Option<libc::pid_t>,
        source_pid: libc::pid_t,
        target_pid: Option<libc::pid_t>,
    ) -> Self {
        Metadata {
            proc_name,
            pid,
            source_pid,
            target_pid,
        }
    }

    pub fn proc_name(&self) -> Option<&str> {
        self.proc_name.as_ref().map(String::as_str)
    }

    pub fn pid(&self) -> Option<libc::pid_t> {
        self.pid.clone()
    }

    pub fn source_pid(&self) -> libc::pid_t {
        self.source_pid
    }

    pub fn target_pid(&self) -> Option<libc::pid_t> {
        self.target_pid.clone()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MessageData {
    meta: Metadata,
    kind: Option<MessageKind>,
}

/// This is not serializable, it's used to cary message metadata
pub struct Message {
    data: MessageData,
    fd: Option<RawFd>,
}

impl Message {
    pub fn my_pid() -> libc::pid_t {
        nix::unistd::Pid::this().as_raw()
    }

    pub fn prepare_message<T: ToMessageKind>(meta: Metadata, data: T) -> Result<Self, Error> {
        let (kind, fd) = data.to_message_kind()?;

        Ok(Self {
            data: MessageData {
                meta,
                kind: Some(kind),
            },
            fd,
        })
    }

    pub async fn send_msg(mut self, target: &mut AsyncCtlEnd<Write>) -> Result<(), Error> {
        println!("sending message");
        let mut cmsg: [RawFd; 1] = [0; 1];
        let fd_to_send = match self
            .data
            .kind
            .as_ref()
            .ok_or_else(|| Error::from("kind must not be None here"))?
        {
            MessageKind::ReadPipeEnd
            | MessageKind::WritePipeEnd
            | MessageKind::ReadCtlEnd
            | MessageKind::WriteCtlEnd => {
                let fd = self
                    .fd
                    .take()
                    .ok_or_else(|| Error::from("no fd in message to send"))?;

                println!("sending fd: {:?}", fd);

                // TODO: smallvec here
                cmsg[0] = fd;
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

            let data: MessageData = bincode::deserialize(recv_iovec_to_recv[0].as_slice())?;

            let fd: Option<RawFd> = match data
                .kind
                .as_ref()
                .ok_or_else(|| Error::from("kind must not be None here"))?
            {
                MessageKind::ReadPipeEnd
                | MessageKind::WritePipeEnd
                | MessageKind::ReadCtlEnd
                | MessageKind::WriteCtlEnd => {
                    let cmsg = msg.cmsgs().next();

                    if let Some(ControlMessageOwned::ScmRights(raw_fd)) = cmsg {
                        // This is safe becuase the FD is being given to and only owned by the PipeEnd
                        Some(
                            *raw_fd
                                .first()
                                .ok_or_else(|| Error::from("no fd associated to message"))?,
                        )
                    } else {
                        return Poll::Ready(Err(Error::from("Missing ScmRights from message")));
                    }
                }
                _ => None,
            };

            let msg = Message { data, fd };
            Poll::Ready(Ok(msg))
        })
        .await?;

        Ok(msg)
    }

    pub fn take_read_pipe_end(&mut self) -> Option<AsyncPipeEnd<Read>> {
        match self.data.kind.as_ref()? {
            MessageKind::ReadPipeEnd => {
                let fd = self.fd.take()?;
                unsafe { PipeEnd::from_raw_fd(fd).into_async_pipe_end().ok() }
            }
            _ => None,
        }
    }

    pub fn take_write_pipe_end(&mut self) -> Option<AsyncPipeEnd<Read>> {
        match self.data.kind.as_ref()? {
            MessageKind::WritePipeEnd => {
                let fd = self.fd.take()?;
                unsafe { PipeEnd::from_raw_fd(fd).into_async_pipe_end().ok() }
            }
            _ => None,
        }
    }

    pub fn take_read_ctl_end(&mut self) -> Option<AsyncCtlEnd<Read>> {
        match self.data.kind.as_ref()? {
            MessageKind::ReadCtlEnd => {
                let fd = self.fd.take()?;
                unsafe { CtlEnd::from_raw_fd(fd).into_async_ctl_end().ok() }
            }
            _ => None,
        }
    }

    pub fn take_write_ctl_end(&mut self) -> Option<AsyncCtlEnd<Write>> {
        match self.data.kind.as_ref()? {
            MessageKind::WriteCtlEnd => {
                let fd = self.fd.take()?;
                unsafe { CtlEnd::from_raw_fd(fd).into_async_ctl_end().ok() }
            }
            _ => None,
        }
    }

    /// Get the message metadata
    pub fn metadata(&self) -> &Metadata {
        &self.data.meta
    }

    pub fn kind(&self) -> Option<&MessageKind> {
        self.data.kind.as_ref()
    }

    pub fn take_kind(&mut self) -> Option<MessageKind> {
        self.data.kind.take()
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        use nix::unistd::close;

        if let Some(fd) = self.fd.take() {
            // FIXME: common close function?
            match fd {
                // don't implicitly close any of the std io
                0..=2 => return,
                // don't close -1, NULL
                i if i < 0 => return,
                _ => (),
            }

            println!("{} warning fd: {}", self.data.meta.source_pid, fd);

            // TODO: need the logger...
            close(fd)
                .map_err(|e| println!("error closing file handle ({}): {}", fd, e))
                .ok();
        }
    }
}
