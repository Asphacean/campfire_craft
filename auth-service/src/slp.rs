//! Hand-rolled Minecraft Server List Ping (protocol 340 — frozen since
//! 2017). ~80 lines against `tokio`, which `campfire-auth` already depends
//! on. RESEARCH.md's legitimacy audit flags both available crates
//! (`craftping`, `mc-server-status`) as low-adoption for a task this small
//! and this frozen — hand-rolling is the ponytail-correct default here, not
//! a corner cut.

use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// MC 1.12.2's protocol number — fixed, this protocol has not changed since
/// the game version this whole project targets.
const PROTOCOL_VERSION: i32 = 340;
/// A hung or slow game server must never hang every `/status` caller
/// (T-03-01-09) — a timeout is an ordinary offline result, not an error.
const SLP_TIMEOUT: Duration = Duration::from_secs(5);

/// The subset of the raw SLP response `/status` actually needs. Discards
/// everything else — in particular `modinfo.modList` (162 entries, ~7.2kB
/// on this server), which the handler must never forward (T-03-01-10).
pub struct PingResult {
    pub players_online: u32,
    pub players_max: u32,
    pub motd: String,
}

fn write_varint(buf: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// Reads a VarInt one byte at a time — the wire format is a byte stream,
/// not a fixed-width int, so there is no faster correct read here.
async fn read_varint(stream: &mut TcpStream) -> std::io::Result<i32> {
    let mut result: i32 = 0;
    let mut shift = 0u32;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        result |= ((byte[0] & 0x7F) as i32) << shift;
        if byte[0] & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 35 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "VarInt too long",
            ));
        }
    }
    Ok(result)
}

async fn read_exact_n(stream: &mut TcpStream, n: usize) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

/// Performs one Server List Ping against `addr` (`host:port`), wrapped in a
/// 5-second timeout. Returns `None` for a timeout, a connection failure, or
/// any parse failure — all ordinary offline results, never propagated as an
/// error (D-11).
pub async fn ping(addr: &str) -> Option<PingResult> {
    let (host, port_str) = addr.rsplit_once(':')?;
    let port: u16 = port_str.parse().ok()?;

    tokio::time::timeout(SLP_TIMEOUT, async {
        let mut stream = TcpStream::connect(addr).await.ok()?;

        // Handshake packet (id 0x00): protocol version, server address,
        // server port, next state (1 = status).
        let mut handshake = Vec::new();
        write_varint(&mut handshake, 0x00);
        write_varint(&mut handshake, PROTOCOL_VERSION);
        write_varint(&mut handshake, host.len() as i32);
        handshake.extend_from_slice(host.as_bytes());
        handshake.extend_from_slice(&port.to_be_bytes());
        write_varint(&mut handshake, 1);

        let mut framed = Vec::new();
        write_varint(&mut framed, handshake.len() as i32);
        framed.extend_from_slice(&handshake);
        stream.write_all(&framed).await.ok()?;

        // Status Request packet: length 1, packet id 0x00, empty body.
        stream.write_all(&[0x01, 0x00]).await.ok()?;

        // Response: outer length varint, packet id varint, string length
        // varint, then exactly that many bytes. The real response here is
        // ~7.2kB and spans more than one TCP segment — a single `.read()`
        // call is wrong; a read-exactly loop is required.
        read_varint(&mut stream).await.ok()?; // total length, unused
        let _packet_id = read_varint(&mut stream).await.ok()?;
        let str_len = read_varint(&mut stream).await.ok()? as usize;
        let body = read_exact_n(&mut stream, str_len).await.ok()?;

        let value: Value = serde_json::from_slice(&body).ok()?;

        // `description` is either a plain string or `{"text": "..."}` —
        // this exact server sends the object form (RESEARCH.md Pitfall 1).
        // Never assume a typed `description: String` field will deserialize.
        let motd = match value.get("description") {
            Some(Value::String(s)) => s.clone(),
            Some(other) => other
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            None => String::new(),
        };

        let players_online = value.get("players")?.get("online")?.as_u64()? as u32;
        let players_max = value.get("players")?.get("max")?.as_u64()? as u32;

        Some(PingResult {
            players_online,
            players_max,
            motd,
        })
    })
    .await
    .ok()?
}
