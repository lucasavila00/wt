use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::io::{Read, Write};

const MAX_PACKET_LENGTH: usize = 65_520;
const MAX_COMMAND_SECTION: usize = 1024 * 1024;

pub(crate) fn copy_packet_section(mut from: impl Read, mut to: impl Write) -> Result<()> {
    loop {
        let packet = read_packet(&mut from)?;
        to.write_all(&packet).context("forward Git packet")?;
        if packet == b"0000" {
            to.flush().context("flush Git packet section")?;
            return Ok(());
        }
    }
}

pub(crate) fn read_packet_section(mut from: impl Read) -> Result<Vec<u8>> {
    let mut section = Vec::new();
    loop {
        let packet = read_packet(&mut from)?;
        if section.len() + packet.len() > MAX_COMMAND_SECTION {
            bail!("Git push command section is too large");
        }
        let done = packet == b"0000";
        section.extend_from_slice(&packet);
        if done {
            return Ok(section);
        }
    }
}

fn read_packet(from: &mut impl Read) -> Result<Vec<u8>> {
    let mut header = [0_u8; 4];
    from.read_exact(&mut header)
        .context("read Git packet header")?;
    let header_text = std::str::from_utf8(&header).context("decode Git packet header")?;
    let length = usize::from_str_radix(header_text, 16).context("parse Git packet length")?;
    if matches!(length, 0..=2) {
        return Ok(header.to_vec());
    }
    if !(4..=MAX_PACKET_LENGTH).contains(&length) {
        bail!("invalid Git packet length {length}");
    }
    let mut packet = vec![0; length];
    packet[..4].copy_from_slice(&header);
    from.read_exact(&mut packet[4..])
        .context("read Git packet payload")?;
    Ok(packet)
}

pub(crate) fn packet_lines(section: &[u8]) -> Result<impl Iterator<Item = &[u8]>> {
    let mut offset = 0;
    let mut lines = Vec::new();
    while offset + 4 <= section.len() {
        let length = usize::from_str_radix(
            std::str::from_utf8(&section[offset..offset + 4]).context("decode Git packet")?,
            16,
        )
        .context("parse Git packet")?;
        offset += 4;
        if length == 0 {
            break;
        }
        if length < 4 || offset + length - 4 > section.len() {
            bail!("invalid Git packet section");
        }
        lines.push(&section[offset..offset + length - 4]);
        offset += length - 4;
    }
    Ok(lines.into_iter())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushUpdate {
    pub previous_oid: String,
    pub new_oid: String,
    pub reference: String,
}

pub(crate) fn push_commands(section: &[u8]) -> Result<Vec<PushUpdate>> {
    let mut offset = 0;
    let mut commands = Vec::new();
    while offset < section.len() {
        if section.len() - offset < 4 {
            bail!("truncated Git push command");
        }
        let length = usize::from_str_radix(
            std::str::from_utf8(&section[offset..offset + 4]).context("decode push packet")?,
            16,
        )
        .context("parse push packet")?;
        offset += 4;
        if length == 0 {
            break;
        }
        if length < 4 || offset + length - 4 > section.len() {
            bail!("invalid Git push command packet");
        }
        let payload = &section[offset..offset + length - 4];
        offset += length - 4;
        let payload = payload.split(|byte| *byte == 0).next().unwrap_or(payload);
        let line = std::str::from_utf8(payload).context("decode Git push command")?;
        let mut fields = line.trim_end_matches('\n').split_whitespace();
        let old = fields.next().context("push command has no old object")?;
        let new = fields.next().context("push command has no new object")?;
        let reference = fields.next().context("push command has no ref")?;
        if fields.next().is_some() || !valid_object_id(old) || !valid_object_id(new) {
            bail!("invalid Git push command");
        }
        commands.push(PushUpdate {
            previous_oid: old.to_owned(),
            new_oid: new.to_owned(),
            reference: reference.to_owned(),
        });
    }
    if commands.is_empty() {
        bail!("Git push did not contain a ref update");
    }
    Ok(commands)
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn successful_push_updates(
    commands: &[u8],
    response: &[u8],
    sideband: bool,
) -> Result<Vec<PushUpdate>> {
    let report = if sideband {
        let mut report = Vec::new();
        for packet in packet_lines(response)? {
            if packet.first() == Some(&1) {
                report.extend_from_slice(&packet[1..]);
            }
        }
        report
    } else {
        response.to_vec()
    };
    let accepted: BTreeSet<_> = packet_lines(&report)?
        .filter_map(|line| {
            std::str::from_utf8(line)
                .ok()?
                .trim_end()
                .strip_prefix("ok ")
                .map(str::to_owned)
        })
        .collect();
    push_commands(commands)?
        .into_iter()
        .filter(|update| accepted.contains(&update.reference))
        .map(|update| {
            update
                .reference
                .strip_prefix("refs/heads/")
                .context("validated push contains a non-branch ref")?;
            Ok(update)
        })
        .collect()
}

pub(crate) fn reject_push(stream: &mut impl Write, section: &[u8], reason: &str) -> Result<()> {
    let mut report = Vec::new();
    write_packet(&mut report, b"unpack ok\n")?;
    for update in push_commands(section)? {
        write_packet(
            &mut report,
            format!("ng {} {reason}\n", update.reference).as_bytes(),
        )?;
    }
    report.extend_from_slice(b"0000");
    if push_uses_sideband(section)? {
        let mut sideband = Vec::with_capacity(report.len() + 1);
        sideband.push(1);
        sideband.extend_from_slice(&report);
        write_packet(stream, &sideband)?;
        stream.write_all(b"0000").context("write sideband end")?;
    } else {
        stream.write_all(&report).context("write push rejection")?;
    }
    stream.flush().context("flush push rejection")
}

pub fn push_uses_sideband(section: &[u8]) -> Result<bool> {
    if section.len() < 4 {
        bail!("invalid Git push command section");
    }
    let length = usize::from_str_radix(
        std::str::from_utf8(&section[..4]).context("decode push packet")?,
        16,
    )
    .context("parse push packet")?;
    if length < 4 || length > section.len() {
        bail!("invalid Git push command packet");
    }
    let payload = &section[4..length];
    let capabilities = payload
        .iter()
        .position(|byte| *byte == 0)
        .map(|position| &payload[position + 1..])
        .unwrap_or_default();
    Ok(capabilities
        .split(|byte| byte.is_ascii_whitespace())
        .any(|capability| capability == b"side-band-64k" || capability == b"side-band"))
}

pub fn write_packet(to: &mut impl Write, payload: &[u8]) -> Result<()> {
    let length = payload.len() + 4;
    if length > MAX_PACKET_LENGTH {
        bail!("Git packet is too large");
    }
    write!(to, "{length:04x}").context("write Git packet length")?;
    to.write_all(payload).context("write Git packet payload")
}
