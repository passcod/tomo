use crate::{
	parsers::{Indic, Path, CONTAINER_HEADER_SIZE, INDIC_SIZE},
	SourceState, TomoError,
};
use deku::DekuContainerRead;
use futures::{
	stream::Stream,
	task::{Context, Poll},
	AsyncReadExt, Future,
};
use std::pin::Pin;

pub struct IndexStream<'src> {
	source: &'src mut SourceState<'src>,
	container: usize,
	inited: bool,
	bytes_left: u64,
}

impl<'src> IndexStream<'src> {
	pub(crate) fn new(source: &'src mut SourceState<'src>, container: usize) -> Self {
		Self {
			source,
			container,
			inited: false,
			bytes_left: 0,
		}
	}

	async fn init(&mut self) -> Result<Option<()>, TomoError> {
		let (start, index_bytes) = {
			let (start, ref header) = self.source.headers[self.container];
			(start, header.index_bytes)
		};

		if !self.inited {
			self.source
				.seek_to(start + (CONTAINER_HEADER_SIZE as u64))
				.await?;
			self.bytes_left = index_bytes;
			self.inited = true;
		}

		if self.bytes_left <= 0 {
			Ok(None)
		} else {
			Ok(Some(()))
		}
	}

	async fn read_indic(&mut self) -> Result<Indic, TomoError> {
		let mut buf = vec![0_u8; INDIC_SIZE as usize];
		self.bytes_left -= self.source.source.read(&mut buf).await? as u64;
		let (_, indic) = Indic::from_bytes((&buf, 0))?;
		Ok(indic)
	}

	async fn async_poll(&mut self) -> Option<Result<Indic, TomoError>> {
		let ret: Result<Option<Indic>, TomoError> = async {
			if let None = self.init().await? {
				return Ok(None);
			}

			let indic= self.read_indic().await?;
			Ok(Some(indic))
		}
		.await;
		ret.transpose()
	}
}

impl<'src> Stream for IndexStream<'src> {
	type Item = Result<Indic, TomoError>;
	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut fut =
			Box::pin(self.async_poll()) as Pin<Box<dyn Future<Output = Option<Self::Item>>>>;
		Future::poll(fut.as_mut(), cx)
	}
}
