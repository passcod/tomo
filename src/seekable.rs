use futures::{
    io::{Error, SeekFrom},
    task::{Context, Poll},
    AsyncRead, AsyncSeek,
};
use std::pin::Pin;

pub trait SeekableSource: AsyncRead + AsyncSeek {}

pub struct Seekable<'t, T: AsyncRead + AsyncSeek + Unpin> {
    pub source: &'t mut T,
}

impl<'t, T: AsyncRead + AsyncSeek + Unpin> Seekable<'t, T> {
    pub fn new(source: &'t mut T) -> Self {
        Self { source }
    }
}

impl<T: AsyncRead + AsyncSeek + Unpin> AsyncRead for Seekable<'_, T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.source).poll_read(cx, buf)
    }
}

impl<T: AsyncRead + AsyncSeek + Unpin> AsyncSeek for Seekable<'_, T> {
    fn poll_seek(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<Result<u64, Error>> {
        Pin::new(&mut self.source).poll_seek(cx, pos)
    }
}

impl<T: AsyncRead + AsyncSeek + Unpin> SeekableSource for Seekable<'_, T> {}
