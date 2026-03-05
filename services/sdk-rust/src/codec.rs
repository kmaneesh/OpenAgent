use crate::types::Frame;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

pub struct Decoder {
    reader: BufReader<OwnedReadHalf>,
}

impl Decoder {
    pub fn new(read_half: OwnedReadHalf) -> Self {
        Self {
            reader: BufReader::new(read_half),
        }
    }

    pub async fn next_frame(&mut self) -> Result<Option<Frame>> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await.context("Failed to read line from socket")?;
        
        if n == 0 {
            return Ok(None); // EOF
        }

        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }

        let frame: Frame = serde_json::from_str(line).context("Failed to deserialize frame")?;
        Ok(Some(frame))
    }
}

pub struct Encoder {
    writer: OwnedWriteHalf,
}

impl Encoder {
    pub fn new(write_half: OwnedWriteHalf) -> Self {
        Self { writer: write_half }
    }

    pub async fn write_frame(&mut self, frame: &Frame) -> Result<()> {
        let mut data = serde_json::to_vec(frame).context("Failed to serialize frame")?;
        data.push(b'\n');
        self.writer.write_all(&data).await.context("Failed to write to socket")?;
        self.writer.flush().await.context("Failed to flush socket")?;
        Ok(())
    }
}
