// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::fmt::Debug;
use std::marker::PhantomData;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use mio_uds::UnixStream;
use nix;
use nix::sys::socket::{
    self, recvmsg, sendmsg, socketpair, AddressFamily, ControlMessage, MsgFlags, SockAddr,
    SockFlag, SockType,
};
use nix::sys::uio::IoVec;
use nix::unistd::close;
use tokio::io::PollEvented;

use crate::pipe::{End, Read, Write};
use crate::Error;

#[derive(Debug)]
pub struct CtlEnd<E: End> {
    raw_fd: RawFd,
    ghost: PhantomData<E>,
}

impl<E: End> CtlEnd<E> {
    /// Forget the fd so that drop is not called after being associated to STDIN or similar
    pub fn forget(&mut self) {
        self.raw_fd = -1;
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if self.raw_fd < 0 {
            return Ok(());
        }

        Ok(close(self.raw_fd)?)
    }

    pub fn into_async_pipe_end(self) -> Result<AsyncCtlEnd<E>, Error> {
        let fd = self.into_raw_fd();
        // this is safe since we are passing ownership from self to the new UnixStream
        let stream =
            unsafe { UnixStream::from_stream(std::os::unix::net::UnixStream::from_raw_fd(fd)) };

        Ok(AsyncCtlEnd::from_unix_stream(stream?)?)
    }
}

impl<E: End> FromRawFd for CtlEnd<E> {
    unsafe fn from_raw_fd(raw_fd: RawFd) -> Self {
        Self {
            raw_fd,
            ghost: PhantomData,
        }
    }
}

impl<E: End> AsRawFd for CtlEnd<E> {
    fn as_raw_fd(&self) -> RawFd {
        self.raw_fd
    }
}

impl<E: End> IntoRawFd for CtlEnd<E> {
    fn into_raw_fd(mut self) -> RawFd {
        let raw_fd = self.raw_fd;
        self.forget();
        raw_fd
    }
}

// TODO: requires forgetting self when STDIN or SRDOUT are attached to it...
impl<E: End> Drop for CtlEnd<E> {
    fn drop(&mut self) {
        match self.raw_fd {
            // don't implicitly close any of the std io
            0..=2 => return,
            // don't close -1, NULL
            i if i < 0 => return,
            _ => (),
        }

        println!("closing fd: {} ({})", self.raw_fd, E::display());

        // TODO: need the logger...
        close(self.raw_fd)
            .map_err(|e| println!("error closing file handle ({}): {}", self.raw_fd, e))
            .ok();
    }
}

pub struct Control {
    read: CtlEnd<Read>,
    write: CtlEnd<Write>,
}

impl Control {
    /// Creates a new pipe, if possible,
    ///
    /// This should be converted to the specific end desired, i.e. read() will close write() implicitly.
    ///   it's expected that this is created before forking, and then used after forking.
    pub fn new() -> nix::Result<Self> {
        let (read, write) = socketpair(
            AddressFamily::Unix,
            SockType::Stream,
            None,
            SockFlag::empty(),
        )?;

        println!("created socketpair, read: {} write: {}", read, write);

        // This is safe, because the CrlEnds are taking direct ownership of the FileHandles
        unsafe {
            Ok(Self {
                read: CtlEnd::from_raw_fd(read),
                write: CtlEnd::from_raw_fd(write),
            })
        }
    }

    pub fn take_writer(self) -> CtlEnd<Write> {
        let Control { write, .. } = self;
        write
    }

    pub fn take_reader(self) -> CtlEnd<Read> {
        let Control { read, .. } = self;
        read
    }

    // FIXME: maybe unsafe?
    pub fn split(self) -> (CtlEnd<Read>, CtlEnd<Write>) {
        let Control { read, write } = self;
        (read, write)
    }
}

pub struct AsyncCtlEnd<E: End> {
    stream: PollEvented<mio_uds::UnixStream>,
    ghost: PhantomData<E>,
}

impl<E: End> AsyncCtlEnd<E> {
    pub fn from_unix_stream(stream: mio_uds::UnixStream) -> Result<Self, Error> {
        Ok(Self {
            stream: PollEvented::new(stream)?,
            ghost: PhantomData,
        })
    }
}

trait RecvMsg {
    fn recvmsg<'a>(
        &mut self,
        iov: &[IoVec<&mut [u8]>],
        cmsg_buffer: Option<&'a mut Vec<u8>>,
        flags: MsgFlags,
    ) -> Result<socket::RecvMsg<'a>, nix::Error>;
}

impl RecvMsg for UnixStream {
    fn recvmsg<'a>(
        &mut self,
        iov: &[IoVec<&mut [u8]>],
        cmsg_buffer: Option<&'a mut Vec<u8>>,
        flags: MsgFlags,
    ) -> Result<socket::RecvMsg<'a>, nix::Error> {
        recvmsg(self.as_raw_fd(), iov, cmsg_buffer, flags).map_err(|e| e.into())
    }
}

trait SendMsg {
    fn sendmsg(
        &mut self,
        iov: &[IoVec<&[u8]>],
        cmsgs: &[ControlMessage],
        flags: MsgFlags,
        addr: Option<&SockAddr>,
    ) -> Result<usize, nix::Error>;
}

impl SendMsg for UnixStream {
    fn sendmsg(
        &mut self,
        iov: &[IoVec<&[u8]>],
        cmsgs: &[ControlMessage],
        flags: MsgFlags,
        addr: Option<&SockAddr>,
    ) -> Result<usize, nix::Error> {
        sendmsg(self.as_raw_fd(), iov, cmsgs, flags, addr)
    }
}

// TODO: remove these impls once sendmsg/recvmsg are in UnixDomain in Tokio
impl AsyncCtlEnd<Read> {
    pub fn poll_recvmsg<'a>(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut iov: &[IoVec<&mut [u8]>],
        cmsg_buffer: Option<&'a mut Vec<u8>>,
        flags: MsgFlags,
    ) -> Poll<Result<socket::RecvMsg<'a>, Error>> {
        ready!(self.stream.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.stream.get_mut().recvmsg(&mut iov, cmsg_buffer, flags) {
            Err(nix::Error::Sys(nix::errno::EWOULDBLOCK)) => {
                self.stream.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            x => Poll::Ready(x.map_err(|e| e.into())),
        }
    }
}

// TODO: remove these impls once sendmsg/recvmsg are in UnixDomain in Tokio
impl AsyncCtlEnd<Write> {
    pub fn poll_sendmsg(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        iov: &[IoVec<&[u8]>],
        cmsgs: &[ControlMessage],
        flags: MsgFlags,
        addr: Option<&SockAddr>,
    ) -> Poll<Result<usize, Error>> {
        ready!(self.stream.poll_write_ready(cx))?;

        match self.stream.get_mut().sendmsg(iov, cmsgs, flags, addr) {
            Err(nix::Error::Sys(nix::errno::EWOULDBLOCK)) => {
                self.stream.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x.map_err(|e| e.into())),
        }
    }
}
