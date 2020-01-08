// Copyright 2019 Benjamin Fry <benjaminfry@me.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::fmt::Debug;
use std::marker::PhantomData;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::pin::Pin;
use std::task::{Context, Poll};

use nix::unistd::{close, dup2, pipe as nix_pipe};
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncWrite};

#[derive(Clone, Copy, Debug)]
pub struct Read;
#[derive(Clone, Copy, Debug)]
pub struct Write;

// A marker trait to designate the end of the pipe this represents
pub trait End: Clone + Copy + Debug {
    fn display() -> &'static str;
}

impl End for Read {
    fn display() -> &'static str {
        "Read"
    }
}
impl End for Write {
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

    pub fn into_async_pipe_end(self) -> nix::Result<AsyncPipeEnd<E>> {
        let fd = self.into_raw_fd();
        // this is safe since we are passing ownership from self to the new UnixStream
        let stream = unsafe { File::from_std(std::fs::File::from_raw_fd(fd)) };

        Ok(AsyncPipeEnd::from_file(stream))
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

        println!("closing fd: {} ({})", self.raw_fd, E::display());

        // TODO: need the logger...
        close(self.raw_fd)
            .map_err(|e| println!("error closing file handle: {}", self.raw_fd))
            .ok();
    }
}

pub struct Pipe {
    read: PipeEnd<Read>,
    write: PipeEnd<Write>,
}

impl Pipe {
    pub(crate) unsafe fn from_raw_fd(read: RawFd, write: RawFd) -> Self {
        Self {
            read: PipeEnd::from_raw_fd(read),
            write: PipeEnd::from_raw_fd(write),
        }
    }

    /// Creates a new pipe, if possible,
    ///
    /// This should be converted to the specific end desired, i.e. read() will close write() implicitly.
    ///   it's expected that this is created before forking, and then used after forking.
    pub fn new() -> nix::Result<Self> {
        let (read, write) = nix_pipe()?;

        println!("created pipe, read: {} write: {}", read, write);

        // sole ownership of the Pipe's file descriptors
        unsafe {
            Ok(Self {
                read: PipeEnd::from_raw_fd(read),
                write: PipeEnd::from_raw_fd(write),
            })
        }
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
    stream: File,
    ghost: PhantomData<E>,
}

impl<E: End> AsyncPipeEnd<E> {
    pub fn from_file(stream: File) -> Self {
        Self {
            stream,
            ghost: PhantomData,
        }
    }
}

impl AsyncRead for AsyncPipeEnd<Read> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<tokio::io::Result<usize>> {
        let pin = Pin::new(&mut self.stream);
        pin.poll_read(cx, buf)
    }
}

impl AsyncWrite for AsyncPipeEnd<Write> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<tokio::io::Result<usize>> {
        let pin = Pin::new(&mut self.stream);
        pin.poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<tokio::io::Result<()>> {
        let pin = Pin::new(&mut self.stream);
        pin.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<tokio::io::Result<()>> {
        let pin = Pin::new(&mut self.stream);
        pin.poll_shutdown(cx)
    }
}