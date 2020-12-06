use crate::{
	parsers::{EntryHeader, Indic, IndicKind, Lookup, Path, CONTAINER_HEADER_SIZE, LOOKUP_SIZE},
	stream::IndexStream,
	SourceState, Tomo, TomoError,
};
use deku::DekuContainerRead;
use futures::{
	stream::{Stream, StreamExt},
	task::{Context, Poll},
	Future,
};
use std::pin::Pin;

pub struct PathsStream<'tomo, 's> {
	tomo: &'tomo mut Tomo<'s>,

	total_in_container: u32,
	path_in_container: u32,
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
		Self {
			tomo,

			total_in_container: 0,
			path_in_container: 0,
		}
	}

	async fn read_lookup(
		&mut self,
		source: &mut SourceState<'_>,
		lookup_offset: u64,
	) -> Result<u64, TomoError> {
		source.seek_to(lookup_offset).await?;
		let bytes = source.read(LOOKUP_SIZE as u64).await?;
		let (_, lookup) = Lookup::from_bytes((&bytes, 0))?;
		Ok(lookup.offset)
	}

	async fn read_path(
		&mut self,
		source: &mut SourceState<'_>,
		path_offset: u64,
		next_offset: u64,
	) -> Result<Path, TomoError> {
		source.seek_to(path_offset).await?;
		let bytes = source.read(next_offset - path_offset).await?;
		let (_, path) = Path::from_bytes((&bytes, 0))?;
		Ok(path)
	}

	async fn next_indic(&mut self) -> Result<(), TomoError> {
		todo!("find next Paths indic");
		self.total_in_container = todo!("read path count");
		self.path_in_container = 0;
		Ok(())
	}

	async fn async_poll(&mut self) -> Option<Result<Path, TomoError>> {
		let ret: Result<Option<Path>, TomoError> = async {
			'retry: loop {
				let source = todo!("obtain source");
				let entry_data_offset: u64 = todo!("obtain entry data offset");
				let lookups_offset: u64 = entry_data_offset + 4;
				let paths_offset: u64 =
					lookups_offset + (LOOKUP_SIZE as u64 * self.total_in_container as u64);

				let path_lookup =
					lookups_offset + (self.path_in_container as u64) * (LOOKUP_SIZE as u64);
				if path_lookup >= paths_offset {
					self.next_indic().await?;
					continue 'retry;
				}
				let path_offset = self.read_lookup(source, path_lookup).await?;

				let next_lookup = path_lookup + (LOOKUP_SIZE as u64);
				let next_offset = if next_lookup >= paths_offset {
					paths_offset
				} else {
					self.read_lookup(source, next_lookup).await?
				};

				let path = self.read_path(source, path_offset, next_offset).await?;

				self.path_in_container += 1;

				break Ok(Some(path));
			}
		}
		.await;
		ret.transpose()
	}

	async fn async_poll0(&mut self) -> Option<Result<Path, TomoError>> {
		let ret: Result<Option<Path>, TomoError> = async {
			Ok(None)
			/*
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
			}*/
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
