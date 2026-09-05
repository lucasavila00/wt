//! Find the end of one pack without waiting for the client's transport EOF.
//! Git index-pack subsequently validates checksums, deltas and object contents.
use anyhow::{bail, Context, Result};
use flate2::{Decompress, FlushDecompress, Status};
use std::io::{BufRead, BufReader, Read, Write};

pub(crate) fn copy_pack(
    input: impl Read,
    output: &mut impl Write,
    hash_bytes: usize,
) -> Result<()> {
    let mut input = BufReader::new(input);
    let mut header = [0; 12];
    input
        .read_exact(&mut header)
        .context("read Git pack header")?;
    if &header[..4] != b"PACK" || !matches!(u32::from_be_bytes(header[4..8].try_into()?), 2 | 3) {
        bail!("invalid Git pack header");
    }
    output.write_all(&header)?;
    let objects = u32::from_be_bytes(header[8..].try_into()?);
    for _ in 0..objects {
        let first = copy_byte(&mut input, output)?;
        let kind = (first >> 4) & 7;
        copy_varint_tail(&mut input, output, first)?;
        match kind {
            1..=4 => (),
            6 => {
                let first = copy_byte(&mut input, output)?;
                copy_varint_tail(&mut input, output, first)?;
            }
            7 => copy_exact(&mut input, output, hash_bytes)?,
            _ => bail!("invalid Git pack object type"),
        }
        let mut decoder = Decompress::new(true);
        let mut decompressed = [0; 8192];
        loop {
            let buffer = input.fill_buf()?;
            let before_in = decoder.total_in();
            let before_out = decoder.total_out();
            let status = decoder
                .decompress(buffer, &mut decompressed, FlushDecompress::None)
                .context("invalid compressed Git object")?;
            let consumed = (decoder.total_in() - before_in) as usize;
            output.write_all(&buffer[..consumed])?;
            input.consume(consumed);
            if status == Status::StreamEnd {
                break;
            }
            if consumed == 0 && decoder.total_out() == before_out {
                bail!("incomplete compressed Git object");
            }
        }
    }
    copy_exact(&mut input, output, hash_bytes)
}

fn copy_byte(input: &mut impl Read, output: &mut impl Write) -> Result<u8> {
    let mut byte = [0];
    input
        .read_exact(&mut byte)
        .context("read Git pack object header")?;
    output.write_all(&byte)?;
    Ok(byte[0])
}

fn copy_varint_tail(input: &mut impl Read, output: &mut impl Write, mut byte: u8) -> Result<()> {
    for _ in 0..10 {
        if byte & 0x80 == 0 {
            return Ok(());
        }
        byte = copy_byte(input, output)?;
    }
    bail!("Git pack object header is too large")
}

fn copy_exact(input: &mut impl Read, output: &mut impl Write, bytes: usize) -> Result<()> {
    if std::io::copy(&mut input.take(bytes as u64), output)? != bytes as u64 {
        bail!("truncated Git pack");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::ZlibEncoder, Compression};

    struct Fragmented<'a>(&'a [u8]);

    impl Read for Fragmented<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            assert!(
                !self.0.is_empty(),
                "must finish the pack without waiting for EOF"
            );
            let count = buffer.len().min(3).min(self.0.len());
            buffer[..count].copy_from_slice(&self.0[..count]);
            self.0 = &self.0[count..];
            Ok(count)
        }
    }

    fn fixture(hash_bytes: usize) -> Vec<u8> {
        let mut pack = b"PACK\0\0\0\x02\0\0\0\x06".to_vec();
        for kind in [1, 2, 3, 4, 6, 7] {
            pack.extend([0x80 | kind << 4, 1]);
            if kind == 6 {
                pack.extend([0x80, 1]);
            } else if kind == 7 {
                pack.extend(vec![1; hash_bytes]);
            }
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&[b'x'; 16]).unwrap();
            pack.extend(encoder.finish().unwrap());
        }
        pack.extend(vec![0; hash_bytes]);
        pack
    }

    #[test]
    fn copies_fragmented_objects_and_delta_headers_without_transport_eof() {
        for hash_bytes in [20, 32] {
            let pack = fixture(hash_bytes);
            let mut copied = Vec::new();
            copy_pack(Fragmented(&pack), &mut copied, hash_bytes).unwrap();
            assert_eq!(copied, pack);
            for end in 0..pack.len() {
                assert!(
                    copy_pack(&pack[..end], &mut Vec::new(), hash_bytes).is_err(),
                    "truncation at {end}"
                );
            }
        }
    }

    #[test]
    fn rejects_malformed_framing() {
        let invalid_header = b"oops\0\0\0\x02\0\0\0\0";
        insta::assert_snapshot!(copy_pack(&invalid_header[..], &mut Vec::new(), 20).unwrap_err().to_string(), @"invalid Git pack header");
        let mut invalid_type = b"PACK\0\0\0\x02\0\0\0\x01".to_vec();
        invalid_type.push(0x50);
        insta::assert_snapshot!(copy_pack(&invalid_type[..], &mut Vec::new(), 20).unwrap_err().to_string(), @"invalid Git pack object type");
        let mut invalid_varint = b"PACK\0\0\0\x02\0\0\0\x01".to_vec();
        invalid_varint.extend([0xb0; 12]);
        insta::assert_snapshot!(copy_pack(&invalid_varint[..], &mut Vec::new(), 20).unwrap_err().to_string(), @"Git pack object header is too large");
    }
}
