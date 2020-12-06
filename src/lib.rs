use deku::DekuContainerRead;
use futures::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};
use parsers::{ContainerHeader, CONTAINER_HEADER_SIZE};
use seekable::{Seekable, SeekableSource};
use std::{fmt, io::SeekFrom};
use thiserror::Error;

pub use stream::PathsStream;

pub mod parsers;
pub mod seekable;
mod stream;

pub mod prelude {
	pub use crate::seekable::{Seekable, SeekableSource};
	pub use crate::{SourceStatus, Tomo, TomoError};
	pub use futures::stream::StreamExt as _;
}

// FIXME: all the `as` conversions really need to be ::from and ::try_from

#[derive(Debug, Default)]
pub struct Tomo<'s> {
	sources: Vec<SourceState<'s>>,
}

pub struct SourceState<'s> {
	source: Box<dyn SeekableSource + 's>,
	offset: u64,
	headers: Vec<(u64, ContainerHeader)>,
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

impl<'s> SourceState<'s> {
	fn new(source: Box<dyn SeekableSource + 's>) -> Self {
		SourceState {
			source,
			offset: 0,
			headers: Vec::new(),
		}
	}

	/// The amount of loaded containers for this source.
	pub fn len(&self) -> usize {
		self.headers.len()
	}

	pub(crate) async fn seek_to(&mut self, target: u64) -> Result<(), TomoError> {
		if target > self.offset {
			let diff = (target - self.offset) as i64;
			self.source.seek(SeekFrom::Current(diff)).await?;
			self.offset = target;
			Ok(())
		} else if target < self.offset {
			let diff = (self.offset - target) as i64;
			self.source.seek(SeekFrom::Current(-diff)).await?;
			self.offset = target;
			Ok(())
		} else {
			Ok(()) // target == self.offset
		}
	}

	pub(crate) async fn read(&mut self, bytes: u64) -> Result<Vec<u8>, TomoError> {
		let mut buf = vec![0; bytes as usize];
		let bytes_read = self.source.read(&mut buf).await? as u64;
		if bytes_read > bytes {
			panic!("read more bytes than would fit in buffer, somehow");
		} else if bytes_read < bytes {
			return Err(TomoError::UnexpectedEof {
				expected: bytes,
				obtained: bytes_read,
			});
		}

		Ok(buf)
	}

	/// Load the next container from this source.
	///
	/// Seeks to the end of the last known container on the source (or nowhere if none have been
	/// loaded yet), then attempts to load a container. If it finds one, it will also probe the
	/// source and return a [`SourceState`] describing whether the source is at its end, or whether
	/// there's more data to go.
	pub async fn load_next_container(&mut self) -> Result<SourceStatus, TomoError> {
		let current_end = self
			.headers
			.last()
			.map(|(start, header)| {
				start + (CONTAINER_HEADER_SIZE as u64) + header.index_bytes + header.entries_bytes
			})
			.unwrap_or(0);

		self.source
			.seek(SeekFrom::Current((current_end - self.offset) as i64))
			.await?;

		let mut buf = vec![0_u8; CONTAINER_HEADER_SIZE];
		self.offset += self.source.read(&mut buf).await? as u64;

		let (_, header) = ContainerHeader::from_bytes((&buf, 0))?;
		let end = header.index_bytes + header.entries_bytes;
		self.headers.push((current_end, header));
		self.offset += end as u64;
		self.source.seek(SeekFrom::Current(end as i64)).await?;

		// As per AsyncSeek documentation:
		//
		//    “A seek beyond the end of a stream is allowed,
		//     but behavior is defined by the implementation.”
		//
		// This is annoying, because it might be that some sources return some kind of io::Error,
		// or return garbage data or even unitialised data (ugh!), but we'll hope that everything
		// is well-behaved and obeys what we test for below:
		//
		// That if the source is at EOF, attempting a read will return immediately, telling us it's
		// read nothing (read().await? == 0), and otherwise we can safely assume there's more data.
		//
		// If we cannot detect EOF, I'm not sure what to do >:(
		let mut past_the_end = vec![0_u8];
		let presumably_not = self.source.read(&mut past_the_end).await? as i64;
		self.source.seek(SeekFrom::Current(-presumably_not)).await?;

		Ok(if presumably_not == 0 {
			SourceStatus::EndOfSource
		} else {
			SourceStatus::MoreToGo
		})
	}

	pub(crate) fn index<'src: 's>(
		&'src mut self,
		container: usize,
	) -> Option<stream::IndexStream<'src>> {
		if container >= self.headers.len() {
			None
		} else {
			Some(stream::IndexStream::new(self, container))
		}
	}

	// pub(crate) fn entries(&self, container: usize) -> stream::EntriesStream<'_> {
	// 	stream::EntriesStream::new(self, container)
	// }
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
		let ss = self.add_source(source);
		while ss.load_next_container().await? == SourceStatus::MoreToGo {}
		Ok(ss)
	}

	/// Load one container from a byte source.
	///
	/// Same as [`Tomo::load`], but stops after a reading a single container. Seeks the source to
	/// the end of the container, but does not parse more than the header upfront.
	///
	/// Returns a shared borrow to the source state created for this source, which can be used to
	/// prompt the state to load another container or extract data from this particular source, and
	/// the [`SourceStatus`] after the first read.
	pub async fn load_one<'slf, T: AsyncRead + AsyncSeek + Unpin>(
		&'slf mut self,
		source: Seekable<'s, T>,
	) -> Result<(&'slf mut SourceState<'s>, SourceStatus), TomoError> {
		let ss = self.add_source(source);
		let st = ss.load_next_container().await?;
		Ok((ss, st))
	}

	/// The amount of loaded containers.
	pub fn len(&self) -> usize {
		self.sources.iter().map(|source| source.len()).sum()
	}

        /// Stream every path in every container.
        ///
        /// Reads every [`parsers::Path`] from the Paths entry in every container for every
        /// source, as a [`Stream`](futures::stream::Stream). Stream order is unspecified.
        ///
        /// Also see [`Tomo::indexed_paths`].
	pub fn all_paths<'tomo>(&'tomo mut self) -> PathsStream<'tomo, 's> {
		PathsStream::new(self)
	}

        /// Stream paths corresponding to every indic in every container.
        ///
        /// Reads every [`parsers::Indic`] from every index in every container and, for those that
        /// do have a path, reads that path from the container's Paths entry. Stream order is
        /// unspecified.
        ///
        /// This may return a different amount of paths than [`Tomo::all_paths`] for two reasons:
        /// 1. paths in the entry that are not referenced in the index (though that's against spec),
        /// 2. several indics can reference the same path (and this stream does not dedupe).
        ///
        /// Also see [`Tomo::all_paths`].
	pub fn indexed_paths<'tomo>(&'tomo mut self) -> PathsStream<'tomo, 's> {
		todo!()
	}

	fn add_source<'slf, T: AsyncRead + AsyncSeek + Unpin>(
		&'slf mut self,
		source: Seekable<'s, T>,
	) -> &'slf mut SourceState<'s> {
		let ss = SourceState::new(Box::new(source));
		let pos = self.sources.len();
		self.sources.push(ss);
		&mut self.sources[pos]
	}
}

#[derive(Error, Debug)]
pub enum TomoError {
	#[error("io error")]
	Io(#[from] std::io::Error),

	#[error("parse error")]
	Parse(#[from] deku::error::DekuError),

	#[error("found non-tomo data at offset {offset:}")]
	NotAContainer { offset: usize },

	#[error("tried to read {expected:} bytes and got {obtained:} bytes (unexpected EOF)")]
	UnexpectedEof { expected: u64, obtained: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceStatus {
	MoreToGo,
	EndOfSource,
}
