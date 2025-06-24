use futures::io::{self, AsyncReadExt, AsyncWriteExt};

pub trait AsyncWriteFixed: AsyncWriteExt {
    async fn write_u16(&mut self, val: u16) -> io::Result<()>
    where
        Self: Unpin,
    {
        let bytes = val.to_le_bytes();
        self.write_all(&bytes).await
    }
}

impl<W: AsyncWriteExt> AsyncWriteFixed for W {}

pub trait AsyncReadFixed: AsyncReadExt {
    async fn read_u16(&mut self) -> io::Result<u16>
    where
        Self: Unpin,
    {
        let mut bytes = [0u8; size_of::<u16>()];
        self.read_exact(&mut bytes).await?;
        Ok(u16::from_le_bytes(bytes))
    }
}

impl<R: AsyncReadExt> AsyncReadFixed for R {}
