use tomo::prelude::*;

#[async_std::test]
async fn load_only() -> Result<(), TomoError> {
    use futures::io::Cursor;
    let buf: Vec<u8> = vec![1, 2, 3];
    let mut reader = Cursor::new(buf);
    let mut tomo = Tomo::default();
    tomo.load(Seekable { inner: &mut reader }).await?;
    Ok(())
}
