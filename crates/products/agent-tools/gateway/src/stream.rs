use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{Read, Write};
use std::net::Shutdown;
use wt_git_smart_protocol::DuplexStream;

const MAX_HEADER: usize = 64 * 1024;

pub fn read_json_line<T: DeserializeOwned>(stream: &mut impl Read) -> Result<T> {
    let mut line = Vec::new();
    while line.len() < MAX_HEADER {
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .context("read request header")?;
        line.push(byte[0]);
        if byte[0] == b'\n' {
            return serde_json::from_slice(&line).context("decode request header");
        }
    }
    bail!("request header exceeds {MAX_HEADER} bytes")
}

pub fn write_json_line<T: Serialize>(stream: &mut impl Write, value: &T) -> Result<()> {
    serde_json::to_writer(&mut *stream, value).context("encode response header")?;
    stream.write_all(b"\n").context("write response header")?;
    stream.flush().context("flush response header")
}

pub fn copy_bidirectional<A: DuplexStream, B: DuplexStream>(mut a: A, mut b: B) -> Result<()> {
    let mut a_read = a.try_clone_stream().context("clone first stream")?;
    let a_write = a.try_clone_stream().context("clone first stream")?;
    let mut b_read = b.try_clone_stream().context("clone second stream")?;
    let b_write = b.try_clone_stream().context("clone second stream")?;
    let a_to_b = std::thread::spawn(move || {
        let result = copy_unbuffered(&mut a_read, &mut b);
        let _ = b_write.shutdown_stream(Shutdown::Write);
        result
    });
    let b_to_a = std::thread::spawn(move || {
        let result = copy_unbuffered(&mut b_read, &mut a);
        let _ = a_write.shutdown_stream(Shutdown::Write);
        result
    });
    tolerate_closed(
        a_to_b
            .join()
            .map_err(|_| anyhow::anyhow!("stream thread panicked"))?,
    )?;
    tolerate_closed(
        b_to_a
            .join()
            .map_err(|_| anyhow::anyhow!("stream thread panicked"))?,
    )?;
    Ok(())
}

fn copy_unbuffered(mut from: impl Read, mut to: impl Write) -> std::io::Result<u64> {
    let mut total = 0;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = from.read(&mut buffer)?;
        if count == 0 {
            return Ok(total);
        }
        to.write_all(&buffer[..count])?;
        total += count as u64;
    }
}

fn tolerate_closed(result: std::io::Result<u64>) -> std::io::Result<()> {
    match result {
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::NotConnected
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}
