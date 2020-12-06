use crate::{stream::IndexStream, parsers::{IndicKind, Indic, Path, EntryHeader, CONTAINER_HEADER_SIZE}, Tomo, TomoError};
use futures::{
	stream::{Stream,StreamExt},
	task::{Context, Poll},
	Future,
};
use std::{pin::Pin};

pub struct PathsStream<'tomo, 's> {
	tomo: &'tomo mut Tomo<'s>,
	source: usize,
	container: usize,
	index: usize,
	indic: Option<Indic>,
	entry: Option<EntryHeader>,
}

impl<'tomo, 's> PathsStream<'tomo, 's> {
	pub(crate) fn new(tomo: &'tomo mut Tomo<'s>) -> Self {
		// let stream =
		// iter(tomo.sources.iter_mut())
		// .flat_map(|source| {
		// 	iter(0..source.len())
		// 		.flat_map(|c| {
		// 			source.index(c)
		// 		})
		// })
		// .filter_map(|indic| async {
		// 	match indic {
		// 		Err(e) => Some(Err(e)),
		// 		Ok(indic) => match indic.kind {
		// 			IndicKind::Paths => Some(Ok(indic)),
		// 			_ => None
		// 		}
		// 	}
		// })
		// ;
		Self { tomo, source: 0, container: 0, index: 0, indic: None, entry: None }
	}

	async fn async_poll(&mut self) -> Option<Result<Path, TomoError>> {
		let ret: Result<Option<Path>, TomoError> = async {
			'retry: loop {
				let source = match self.tomo.sources.get_mut(self.source) {
					Some(s) => s,
					None => return Ok(None),
				};

				let (container_start, header) = match source.headers.get(self.container) {
					Some((s, h)) => (*s, h),
					None => {
						self.source += 1;
						self.container = 0;
						self.index = 0;
						self.indic = None;
						self.entry = None;
						continue 'retry;
					}
				};

				// it's all upside down...



				// let index = match self.index {
				// 	Some(ref mut index) => index,
				// 	None => match source.index(self.container) {
				// 		Some(index) => {
				// 			self.index = 1;
				// 			self.indic = None;
				// 			self.entry = None;
				// 			self.index.as_mut().unwrap()
				// 		},
				// 		None => {
				// 			self.source += 1;
				// 			self.container = 0;
				// 			self.index = 0;
				// 			self.indic = None;
				// 			self.entry = None;
				// 			continue 'retry;
				// 		}
				// 	}
				// };

				// let path_indic = match self.indic {
				// 	Some(indic) => indic,
				// 	None => {
				// 		let indic = 'indic: loop {
				// 			match index.next().await {
				// 				Some(Err(err)) => return Err(err),
				// 				Some(Ok(indic)) => match indic.kind {
				// 					IndicKind::Paths => break 'indic indic,
				// 					_ => continue 'indic,
				// 				},
				// 				None => {
				// 					self.container += 1;
				// 					self.index = None;
				// 					self.indic = None;
				// 					self.entry = None;
				// 					continue 'retry;
				// 				}
				// 			}
				// 		};
				// 		self.indic = Some(indic);
				// 		self.entry = None;
				// 		indic
				// 	},
				// };

				// let entry_header = match self.entry {
				// 	Some(ref entry) => entry,
				// 	None => {
				// 		let entry_start = container_start + (CONTAINER_HEADER_SIZE as u64) + header.index_bytes;
				// 		source.seek_to(entry_start).await?;
				// 		todo!()
				// 	}
				// };

				todo!();
				break Ok(None);
			}
		}
		.await;
		ret.transpose()
	}
}

impl<'tomo, 's> Stream for PathsStream<'tomo, 's> {
	type Item = Result<Path, TomoError>;
	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut fut =
			Box::pin(self.async_poll()) as Pin<Box<dyn Future<Output = Option<Self::Item>>>>;
		Future::poll(fut.as_mut(), cx)
	}
}
