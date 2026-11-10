// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A link across machines: the same messages, over a TCP connection.
//!
//! Each side keeps its usual end of a link; two threads carry the other end
//! over the connection, one each way, in frames of a 4-byte big-endian
//! length and an encoded message. When the connection ends, the channels
//! close, as if the other side had stopped; when a side stops, it closes the
//! connection, both ways.

use std::{
    io::{self, BufReader, BufWriter, Read, Write},
    net::{Shutdown, TcpStream},
    thread,
};

use crate::{GatewayEnd, RegionEnd, ToGateway, ToRegion, in_process};

/// A frame larger than this is refused: no message comes close.
const LARGEST_FRAME: usize = 64 << 20;

/// On the region's machine: the end the region uses, whose gateway is at the
/// other end of `stream`.
pub fn region_end(stream: TcpStream) -> io::Result<RegionEnd> {
    let (gateway, region) = in_process();
    let GatewayEnd { to_region, mut from_region } = gateway;
    let reader = stream.try_clone()?;
    stream.set_nodelay(true)?;
    spawn("osage-link-from-gateway", move || {
        let mut reader = BufReader::new(reader);
        while let Some(message) = read(&mut reader, ToRegion::decode) {
            if to_region.send(message).is_err() {
                break;
            }
        }
        let _ = reader.get_ref().shutdown(Shutdown::Both);
    })?;
    spawn("osage-link-to-gateway", move || {
        let mut writer = BufWriter::new(stream);
        while let Some(message) = from_region.blocking_recv() {
            let more = !from_region.is_empty();
            if write(&mut writer, &message.encode(), more).is_err() {
                break;
            }
        }
        let _ = writer.get_ref().shutdown(Shutdown::Both);
    })?;
    Ok(region)
}

/// On the gateway's machine: the end the gateway uses, whose region is at the
/// other end of `stream`.
pub fn gateway_end(stream: TcpStream) -> io::Result<GatewayEnd> {
    let (gateway, region) = in_process();
    let RegionEnd { from_gateway, to_gateway } = region;
    let reader = stream.try_clone()?;
    stream.set_nodelay(true)?;
    spawn("osage-link-from-region", move || {
        let mut reader = BufReader::new(reader);
        while let Some(message) = read(&mut reader, ToGateway::decode) {
            if to_gateway.send(message).is_err() {
                break;
            }
        }
        let _ = reader.get_ref().shutdown(Shutdown::Both);
    })?;
    spawn("osage-link-to-region", move || {
        let mut writer = BufWriter::new(stream);
        while let Ok(message) = from_gateway.recv() {
            let more = !from_gateway.is_empty();
            if write(&mut writer, &message.encode(), more).is_err() {
                break;
            }
        }
        let _ = writer.get_ref().shutdown(Shutdown::Both);
    })?;
    Ok(gateway)
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> io::Result<()> {
    thread::Builder::new().name(name.to_owned()).spawn(work).map(drop)
}

/// The next message, or `None` when the connection ends or breaks the
/// contract: a frame too large or a message that does not decode ends the
/// link rather than being skipped.
fn read<T>(reader: &mut impl Read, decode: impl Fn(&[u8]) -> anyhow::Result<T>) -> Option<T> {
    let mut length = [0; 4];
    reader.read_exact(&mut length).ok()?;
    let length = u32::from_be_bytes(length) as usize;
    if length > LARGEST_FRAME {
        return None;
    }
    let mut frame = vec![0; length];
    reader.read_exact(&mut frame).ok()?;
    decode(&frame).ok()
}

/// Writes one frame; flushes unless more messages are already waiting.
fn write(writer: &mut impl Write, frame: &[u8], more: bool) -> io::Result<()> {
    writer.write_all(&(frame.len() as u32).to_be_bytes())?;
    writer.write_all(frame)?;
    if !more {
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, time::Duration};

    use osage_proto::{
        components::Controller,
        v0::{Goodbye, Role, ServerMessage, server_message},
    };

    use super::*;
    use crate::{Member, SessionKind};

    #[test]
    fn the_same_messages_cross_a_connection_both_ways() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let region_side = thread::spawn(move || region_end(listener.accept().unwrap().0).unwrap());
        let mut gateway = gateway_end(TcpStream::connect(address).unwrap()).unwrap();
        let region = region_side.join().unwrap();

        let member = Member {
            kind: SessionKind::Bridge,
            role: Role::Player,
            controller: Controller { per_world_id: "bridge-1".to_owned(), ..Default::default() },
            owner: "owner-1".to_owned(),
        };
        let join = ToRegion::Join { session: 7, member };
        gateway.to_region.send(join.clone()).unwrap();
        gateway.to_region.send(ToRegion::Leave { session: 7 }).unwrap();
        let received = region.from_gateway.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(received, join);
        assert_eq!(region.from_gateway.recv_timeout(Duration::from_secs(5)).unwrap(), ToRegion::Leave { session: 7 });

        let goodbye = Goodbye { detail: "over the wire".to_owned(), ..Default::default() };
        let message = ServerMessage { message: Some(server_message::Message::Goodbye(goodbye)) };
        region.to_gateway.send(ToGateway::Joined { session: 7 }).unwrap();
        region.to_gateway.send(ToGateway::Broadcast { message: message.clone() }).unwrap();
        assert_eq!(gateway.from_region.blocking_recv().unwrap(), ToGateway::Joined { session: 7 });
        assert_eq!(gateway.from_region.blocking_recv().unwrap(), ToGateway::Broadcast { message });

        // The region stops: the gateway's channel closes.
        drop(region);
        assert_eq!(gateway.from_region.blocking_recv(), None);
    }
}
