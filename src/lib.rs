use futures::{AsyncRead, AsyncSeek};
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
    containers: Vec<ContainerState<'s>>,
}

struct ContainerState<'s> {
    source: Box<dyn SeekableSource + 's>,
    header: Option<ContainerHeader>,
}

impl fmt::Debug for ContainerState<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContainerState")
            .field("stream", &"<boxed async reader>")
            .field("header", &self.header)
            .finish()
    }
}

impl<'s> Tomo<'s> {
    /// Load one or more containers from a byte source.
    ///
    /// Reads a seekable byte source into the Tomo state. Only parses the bare minimum it requires
    /// upfront, and will seek through on demand.
    pub async fn load<T: AsyncRead + AsyncSeek + Unpin>(
        &mut self,
        source: Seekable<'s, T>,
    ) -> Result<(), TomoError> {
        self.containers.push(ContainerState {
            source: Box::new(source),
            header: None,
        });
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum TomoError {}
