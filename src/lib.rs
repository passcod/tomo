use deku::DekuContainerRead;
use futures::{AsyncRead, AsyncReadExt, AsyncSeek};
use parsers::ContainerHeader;
use seekable::{Seekable, SeekableSource};
use std::fmt;
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

struct SourceState<'s> {
    source: Box<dyn SeekableSource + 's>,
    offset: usize,
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
    /// source that may have that behaviour with e.g. a timeout.
    ///
    /// Tomo keeps track of the offset it seeks and reads at internally, and only does relative
    /// seeks, so the source can be already seeked to a position and it will never look back before
    /// that. This is useful when concatenating Tomo archives to other file types. However, Tomo
    /// expects the source to contain containers: it will not attempt to discover them by reading
    /// the source until it finds a Tomo magic, and it stop with a [`TomoError::NotAContainer`]
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
    pub async fn load<T: AsyncRead + AsyncSeek + Unpin>(
        &mut self,
        mut source: Seekable<'s, T>,
    ) -> Result<(), TomoError> {
        let mut buf = vec![0_u8; parsers::CONTAINER_HEADER_SIZE];
        let offset = source.read(&mut buf).await?;
        let mut headers = Vec::new();

        let ((rest, _), header) = ContainerHeader::from_bytes((&buf, 0))?;
        headers.push(header);

        let source = Box::new(source);
        self.sources.push(SourceState { source, offset, headers });
        Ok(())
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
