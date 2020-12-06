use eyre::Result;
use futures::io::Cursor;
use tomo::parsers;
use tomo::prelude::*;

fn empty() -> Vec<u8> {
	let mut data = Vec::new();
	data.extend(&parsers::MAGIC);
	data.push(parsers::Mode::Stacked as u8);
	data.extend(&0_u64.to_le_bytes());
	data.extend(&0_u64.to_le_bytes());
	data
}

#[async_std::test]
async fn only_one() -> Result<()> {
	let mut reader = Cursor::new(empty());
	let mut tomo = Tomo::default();
	let ss = tomo.load(Seekable::new(&mut reader)).await?;

	assert_eq!(ss.len(), 1);
	if let Some(path) = tomo.paths().next().await {
		let path = path?;
		// assert!...
	} else {
		assert!(false, "expected a path");
	}

	Ok(())
}

#[async_std::test]
async fn one_then_another() -> Result<()> {
	let mut double = empty();
	double.extend(empty());
	let mut reader = Cursor::new(double);

	let mut tomo = Tomo::default();
	let (ss, st) = tomo.load_one(Seekable::new(&mut reader)).await?;
	assert_eq!(ss.len(), 1);
	assert_eq!(st, SourceStatus::MoreToGo);
	ss.load_next_container().await?;
	assert_eq!(ss.len(), 2);

	Ok(())
}

#[async_std::test]
async fn two_at_once() -> Result<()> {
	let mut double = empty();
	double.extend(empty());
	let mut reader = Cursor::new(double);

	let mut tomo = Tomo::default();
	let ss = tomo.load(Seekable::new(&mut reader)).await?;
	assert_eq!(ss.len(), 2);

	Ok(())
}
