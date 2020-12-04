use futures::io::Cursor;
use tomo::parsers;
use tomo::prelude::*;

#[async_std::test]
async fn empty() -> Result<(), TomoError> {
    let mut data = Vec::new();
    data.extend(&parsers::MAGIC);
    data.push(parsers::Mode::Stacked as u8);
    data.extend(&0_u64.to_le_bytes());
    data.extend(&0_u64.to_le_bytes());

    let mut reader = Cursor::new(data);
    let mut tomo = Tomo::default();
    tomo.load(Seekable::new(&mut reader)).await?;

    Ok(())
}
