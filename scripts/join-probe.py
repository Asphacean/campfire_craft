#!/usr/bin/env python3
"""Dependency-free Minecraft 1.12.2 (protocol 340) login probe.

Speaks just enough of the vanilla protocol to trigger a login against a
Forge 1.12.2 server and report whatever disconnect it receives - in the
login state or the play state. A test instrument for scripts/devserver.sh's
proofs (and later, 02-03's live-server check and future regression checks)
- not a general client library, kept small and obvious.

Deliberately sends no FML marker on the handshake address string, so at the
protocol layer this looks like a genuine vanilla client. campfire-auth's
`acceptableRemoteVersions="*"` is exactly what lets such a client past
Forge's own FML handshake and into the mod's own join-gate logic, where the
freeze/timeout/kick this probe is testing actually happens.

Usage: join-probe.py <host> <port> <nick> [token]

Without a token: never answers the mod's AuthRequestMessage - the vanilla/
launcher-less client case (AUTH-04's core case, reason=no_packet).
With a token: answers the request packet on the "campfireauth" channel with
{nick, token}, exercising the positive/replay/service-down paths too, if
Forge actually delivers the reply on this connection (see NO_ROUND_TRIP
below - it may not, depending on how Forge classifies a client with no FML
marker at all).

Exit codes:
  0  a disconnect was received (either login-state or play-state); the
     reason is printed
  1  no disconnect was received before the timeout (NO_DISCONNECT) - this
     is the expected outcome for a successfully validated ("allow") join
  3  a token was supplied but no campfireauth request packet ever arrived
     to reply to (NO_ROUND_TRIP) - Forge did not treat this connection as
     one it would hand our packet to
  2  usage error
"""
import socket
import struct
import sys
import time

NEXT_STATE_LOGIN = 2
DISCONNECT_LOGIN = 0x00
LOGIN_SUCCESS = 0x02
SET_COMPRESSION = 0x03
DISCONNECT_PLAY = 0x1A
PLUGIN_MESSAGE_PLAY_CLIENTBOUND = 0x18
PLUGIN_MESSAGE_PLAY_SERVERBOUND = 0x09
AUTH_CHANNEL = "campfireauth"
PROTOCOL_VERSION = 340
OVERALL_TIMEOUT_SECONDS = 10


def write_varint(value):
    out = bytearray()
    while True:
        b = value & 0x7F
        value >>= 7
        if value:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def write_string(s):
    data = s.encode("utf-8")
    return write_varint(len(data)) + data


def read_all(sock, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("connection closed while reading")
        buf.extend(chunk)
    return bytes(buf)


def read_varint(sock):
    value = 0
    position = 0
    while True:
        b = read_all(sock, 1)[0]
        value |= (b & 0x7F) << position
        if not (b & 0x80):
            break
        position += 7
        if position >= 35:
            raise ValueError("VarInt too big")
    if value & 0x80000000:
        value -= 0x100000000
    return value


def read_varint_from_bytes(buf, offset):
    value = 0
    position = 0
    while True:
        b = buf[offset]
        offset += 1
        value |= (b & 0x7F) << position
        if not (b & 0x80):
            break
        position += 7
    return value, offset


def read_string(buf, offset):
    length, offset = read_varint_from_bytes(buf, offset)
    s = buf[offset:offset + length].decode("utf-8", errors="replace")
    return s, offset + length


def send_packet(sock, packet_id, data=b""):
    body = write_varint(packet_id) + data
    sock.sendall(write_varint(len(body)) + body)


def read_packet(sock):
    length = read_varint(sock)
    body = read_all(sock, length)
    packet_id, offset = read_varint_from_bytes(body, 0)
    return packet_id, body[offset:]


def main():
    if len(sys.argv) < 4:
        print("usage: join-probe.py <host> <port> <nick> [token]", file=sys.stderr)
        return 2
    host = sys.argv[1]
    port = int(sys.argv[2])
    nick = sys.argv[3]
    token = sys.argv[4] if len(sys.argv) > 4 else None

    sock = socket.create_connection((host, port), timeout=OVERALL_TIMEOUT_SECONDS)
    sock.settimeout(OVERALL_TIMEOUT_SECONDS)

    handshake = (
        write_varint(PROTOCOL_VERSION)
        + write_string(host)
        + struct.pack(">H", port)
        + write_varint(NEXT_STATE_LOGIN)
    )
    send_packet(sock, 0x00, handshake)
    send_packet(sock, 0x00, write_string(nick))

    deadline = time.time() + OVERALL_TIMEOUT_SECONDS
    state = "login"
    responded_token = False

    while time.time() < deadline:
        try:
            packet_id, data = read_packet(sock)
        except (socket.timeout, ConnectionError):
            break

        if state == "login":
            if packet_id == DISCONNECT_LOGIN:
                reason, _ = read_string(data, 0)
                print(f"disconnect(login): {reason}")
                return 0
            if packet_id == SET_COMPRESSION:
                continue
            if packet_id == LOGIN_SUCCESS:
                state = "play"
                continue
            continue

        # state == "play"
        if packet_id == DISCONNECT_PLAY:
            reason, _ = read_string(data, 0)
            print(f"disconnect(play): {reason}")
            return 0

        if packet_id == PLUGIN_MESSAGE_PLAY_CLIENTBOUND and token is not None and not responded_token:
            channel, _ = read_string(data, 0)
            if channel == AUTH_CHANNEL:
                # Forge's SimpleNetworkWrapper (FMLIndexedMessageToMessageCodec)
                # prepends a 1-byte discriminator - the ID this message type
                # was registered with (AuthResponseMessage = 1 in
                # NetworkHandler) - before the message's own encoded bytes.
                # Omitting it makes the server misread the nick-length varint
                # as the discriminator and drop the connection.
                reply = bytes([1]) + write_string(nick) + write_string(token)
                payload = write_string(AUTH_CHANNEL) + reply
                send_packet(sock, PLUGIN_MESSAGE_PLAY_SERVERBOUND, payload)
                responded_token = True
            continue

        # Any other play-state packet (join game, chunk data, keep-alive,
        # unrelated plugin channels) is irrelevant to this probe.
        continue

    if token is not None and not responded_token:
        print("NO_ROUND_TRIP: never received a campfireauth request packet before timeout")
        return 3

    print("NO_DISCONNECT: no disconnect received within timeout")
    return 1


if __name__ == "__main__":
    sys.exit(main())
