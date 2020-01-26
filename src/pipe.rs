// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::fmt::Debug;
use std::io;
use std::marker::PhantomData;
use std::mem;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use mio;
use mio::event::Evented;
use mio::unix::EventedFd;
use nix::unistd::{close, dup2, pipe as nix_pipe, read as nix_read, write as nix_write};
use tokio::io::PollEvented;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::{ChildStderr, ChildStdout};

#[derive(Clone, Copy, Debug)]
pub struct Read;
#[derive(Clone, Copy, Debug)]
pub struct Write;

// A marker trait to designate the end of the pipe this represents
pub trait End: Clone + Copy + Debug {
    type Reverse: End;

    fn display() -> &'static str;
}

impl End for Read {
    type Reverse = Write;

    fn display() -> &'static str {
        "Read"
    }
}
impl End for Write {
    type Reverse = Read;

    fn display() -> &'static str {
        "Write"
    }
}

#[derive(Debug)]
pub struct PipeEnd<E: End> {
    raw_fd: RawFd,
    ghost: PhantomData<E>,
}

impl<E: End> PipeEnd<E> {
    /// Forget the fd so that drop is not called after being associated to STDIN or similar
    pub fn forget(&mut self) {
        self.raw_fd = -1;
    }

    pub fn close(&mut self) -> nix::Result<()> {
        if self.raw_fd < 0 {
            return Ok(());
        }

        close(self.raw_fd)
    }

    /// Duplicates the filedescriptor
    ///
    /// This will close the target_fd before duping, then dup to the new target and return the new Pipe.
    pub fn dup(&self, target_fd: RawFd) -> nix::Result<Self> {
        close(target_fd)?;
        let new_fd = dup2(self.raw_fd, target_fd)?;

        assert_eq!(new_fd, target_fd);
        Ok(PipeEnd {
            raw_fd: new_fd,
            ghost: PhantomData,
        })
    }

    /// Duplicates and moves the filedescriptor
    ///
    /// This will close the target_fd before duping, then dup to the new target and return the new Pipe.
    pub fn replace(mut self, target_fd: RawFd) -> nix::Result<()> {
        // don't do anything if these are the same file descriptors
        if self.raw_fd == target_fd {
            self.forget();
            return Ok(());
        }

        // first close the target
        if self.raw_fd == -1 {
            return Ok(());
        }
        close(target_fd)?;

        let new_fd = dup2(self.raw_fd, target_fd)?;

        assert_eq!(new_fd, target_fd);
        self.forget();

        Ok(())
    }

    pub fn into_async_pipe_end(self) -> Result<AsyncPipeEnd<E>, crate::Error> {
        // this is safe since we are passing ownership from self to the new UnixStream
        AsyncPipeEnd::from_pipe_end(self)
    }
}

impl<E: End> FromRawFd for PipeEnd<E> {
    unsafe fn from_raw_fd(raw_fd: RawFd) -> Self {
        Self {
            raw_fd,
            ghost: PhantomData,
        }
    }
}

impl<E: End> AsRawFd for PipeEnd<E> {
    fn as_raw_fd(&self) -> RawFd {
        self.raw_fd
    }
}

impl<E: End> IntoRawFd for PipeEnd<E> {
    fn into_raw_fd(mut self) -> RawFd {
        let raw_fd = self.raw_fd;
        self.forget();
        raw_fd
    }
}

// TODO: requires forgetting self when STDIN or SRDOUT are attached to it...
impl<E: End> Drop for PipeEnd<E> {
    fn drop(&mut self) {
        match self.raw_fd {
            // don't implicitly close any of the std io
            0..=2 => return,
            // don't close -1, NULL
            i if i < 0 => return,
            _ => (),
        }

        println!(
            "{} closing fd: {} ({})",
            nix::unistd::Pid::this(),
            self.raw_fd,
            E::display()
        );

        // TODO: need the logger...
        close(self.raw_fd)
            .map_err(|e| println!("error closing file handle ({}): {}", self.raw_fd, e))
            .ok();
    }
}

impl<E: End> Evented for PipeEnd<E> {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}

impl io::Read for PipeEnd<Read> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        nix_read(self.as_raw_fd(), buf).map_err(|e| match e.as_errno() {
            Some(errno) => errno.into(),
            _ => io::Error::new(io::ErrorKind::Other, "unknown nix error"),
        })
    }
}

impl io::Write for PipeEnd<Write> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        nix_write(self.as_raw_fd(), buf).map_err(|e| match e.as_errno() {
            Some(errno) => errno.into(),
            _ => io::Error::new(io::ErrorKind::Other, "unknown nix error"),
        })
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        // FIXME: nix doesn't have flush???
        //nix_flush(self.fd).map(Into::into)
        Ok(())
    }
}

impl From<ChildStdout> for PipeEnd<Read> {
    fn from(stdout: ChildStdout) -> Self {
        let raw_fd = stdout.as_raw_fd();
        mem::forget(stdout);
        unsafe { PipeEnd::from_raw_fd(raw_fd) }
    }
}

impl From<ChildStderr> for PipeEnd<Read> {
    fn from(stdout: ChildStderr) -> Self {
        let raw_fd = stdout.as_raw_fd();
        mem::forget(stdout);
        unsafe { PipeEnd::from_raw_fd(raw_fd) }
    }
}

pub struct Pipe {
    read: PipeEnd<Read>,
    write: PipeEnd<Write>,
}

impl Pipe {
    pub(crate) unsafe fn from_raw_fd(read: RawFd, write: RawFd) -> nix::Result<Self> {
        use nix::fcntl::{fcntl, FcntlArg, OFlag};

        // set the FD as non-blocking
        assert_eq!(
            fcntl(read, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).unwrap(),
            0
        );
        // set the FD as non-blocking
        assert_eq!(
            fcntl(write, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).unwrap(),
            0
        );

        Ok(Self {
            read: PipeEnd::from_raw_fd(read),
            write: PipeEnd::from_raw_fd(write),
        })
    }

    /// Creates a new pipe, if possible,
    ///
    /// This should be converted to the specific end desired, i.e. read() will close write() implicitly.
    ///   it's expected that this is created before forking, and then used after forking.
    pub fn new() -> nix::Result<Self> {
        let (read, write) = nix_pipe()?;
        println!("created pipe, read: {} write: {}", read, write);

        unsafe { Self::from_raw_fd(read, write) }
    }

    pub fn take_writer(self) -> PipeEnd<Write> {
        let Pipe { write, .. } = self;
        write
    }

    pub fn take_reader(self) -> PipeEnd<Read> {
        let Pipe { read, .. } = self;
        read
    }

    // TODO: is this safe?
    pub fn split(self) -> (PipeEnd<Read>, PipeEnd<Write>) {
        (self.read, self.write)
    }
}

pub struct AsyncPipeEnd<E: End> {
    pipe: PollEvented<PipeEnd<E>>,
    ghost: PhantomData<E>,
}

impl<E: End> AsyncPipeEnd<E> {
    pub fn from_pipe_end(pipe: PipeEnd<E>) -> Result<Self, crate::Error> {
        Ok(Self {
            pipe: PollEvented::new(pipe)?,
            ghost: PhantomData,
        })
    }
}

impl AsyncRead for AsyncPipeEnd<Read> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<tokio::io::Result<usize>> {
        let pipe = Pin::new(&mut self.pipe);
        pipe.poll_read(cx, buf)
    }
}

impl AsyncWrite for AsyncPipeEnd<Write> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<tokio::io::Result<usize>> {
        let pipe = Pin::new(&mut self.pipe);
        pipe.poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<tokio::io::Result<()>> {
        let pipe = Pin::new(&mut self.pipe);
        pipe.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<tokio::io::Result<()>> {
        let pipe = Pin::new(&mut self.pipe);
        pipe.poll_shutdown(cx)
    }
}
