use deku::DekuContainerRead;
use futures::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use parsers::ContainerHeader;
use seekable::{Seekable, SeekableSource};
use std::{fmt, io::SeekFrom};
use thiserror::Error;

pub mod parsers;
pub mod seekable;

pub mod prelude {
    pub use crate::seekable::{Seekable, SeekableSource};
    pub use crate::{Tomo, TomoError};
}

#[derive(Debug, Default)]
pub struct Tomo<'s> {
    sources: Vec<SourceState<'s>>,
}

pub struct SourceState<'s> {
    source: Box<dyn SeekableSource + 's>,
    offset: u64,
    headers: Vec<ContainerHeader>,
}

impl fmt::Debug for SourceState<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceState")
            .field("stream", &"<boxed async reader>")
            .field("offset", &self.offset)
            .field("headers", &self.headers)
            .finish()
    }
}

impl<'s> Tomo<'s> {
    /// Load one or more containers from a byte source.
    ///
    /// Reads a seekable byte source into the Tomo state. Only parses the bare minimum it requires
    /// upfront, and will seek through on demand for further operations, thus it takes an exclusive
    /// borrow on the source.
    ///
    /// The source will be parsed as far as it can, and any container headers found added to the
    /// state. This may pause indefinitely if the source is waiting for more data and none is
    /// forthcoming (e.g. a stalled network fetch). It's therefore recommended to preprocess a
    /// source that may have that behaviour with e.g. a timeout, or to use [`Tomo::load_one`] to
    /// stop at the first container.
    ///
    /// Tomo keeps track of the offset it seeks and reads at internally, and only does relative
    /// seeks, so the source can be already seeked to a position and it will never look back before
    /// that. This is useful when concatenating Tomo archives to other file types. However, Tomo
    /// expects the source to contain containers: it will not attempt to discover them by reading
    /// the source until it finds a Tomo magic, and will stop with a [`TomoError::NotAContainer`]
    /// error if/when it finds non-tomo data.
    ///
    /// The byte source needs to be wrapped in a [`Seekable`]:
    ///
    /// ```
    /// # #[async_std::main]
    /// # async fn main() -> Result<(), tomo::prelude::TomoError> {
    /// # use tomo::parsers;
    /// # use futures::io::Cursor;
    /// # let mut data = Vec::new();
    /// # data.extend(&parsers::MAGIC);
    /// # data.push(parsers::Mode::Stacked as u8);
    /// # data.extend(&0_u64.to_le_bytes());
    /// # data.extend(&0_u64.to_le_bytes());
    /// # let mut source = Cursor::new(data);
    /// use tomo::prelude::*;
    /// let mut tomo = Tomo::default();
    /// tomo.load(Seekable::new(&mut source)).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Returns a shared borrow to the source state created for this source, which can be used to
    /// prompt the state to load or extract data from this particular source.
    pub async fn load<'slf, T: AsyncRead + AsyncSeek + Unpin>(
        &'slf mut self,
        source: Seekable<'s, T>,
    ) -> Result<&'slf SourceState<'s>, TomoError> {
        // todo: parse more
        self.load_one(source).await
    }

    /// Load one container from a byte source.
    ///
    /// Same as [`Tomo::load`], but stops after a reading a single container. Seeks the source to
    /// the end of the container, but does not parse more than the header upfront.
    ///
    /// Returns a shared borrow to the source state created for this source, which can be used to
    /// prompt the state to load another container or extract data from this particular source.
    pub async fn load_one<'slf, T: AsyncRead + AsyncSeek + Unpin>(
        &'slf mut self,
        mut source: Seekable<'s, T>,
    ) -> Result<&'slf SourceState<'s>, TomoError> {
        let mut buf = vec![0_u8; parsers::CONTAINER_HEADER_SIZE];
        let mut offset = source.read(&mut buf).await? as u64;
        let mut headers = Vec::new();

        let ((rest, _), header) = ContainerHeader::from_bytes((&buf, 0))?;

        let end = header.index_bytes + header.entries_bytes;
        offset += end as u64;
        source.seek(SeekFrom::Current(end as i64)).await?;

        headers.push(header);
        let source = Box::new(source);
        self.sources.push(SourceState { source, offset, headers });
        Ok(&self.sources[self.sources.len() - 1])
    }
}

#[derive(Error, Debug)]
pub enum TomoError {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("parse error")]
    Parse(#[from] deku::error::DekuError),

    #[error("found non-tomo data at offset {offset:}")]
    NotAContainer { offset: usize }
}
