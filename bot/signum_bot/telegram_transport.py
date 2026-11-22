from __future__ import annotations

import asyncio
import hashlib
import hmac
import os
import secrets
import time

import aiohttp
from telethon.network.connection import ConnectionTcpAbridged


class WebsocketWriter:
    def __init__(self, session, socket, reader):
        self.session = session
        self.socket = socket
        self.reader = reader
        self.pending = bytearray()
        self.closed = None
        self.receiving = asyncio.create_task(self.receive())

    async def receive(self):
        try:
            async for message in self.socket:
                if message.type == aiohttp.WSMsgType.BINARY:
                    self.reader.feed_data(message.data)
                elif message.type == aiohttp.WSMsgType.ERROR:
                    raise ConnectionError("Telegram transport closed")
        except Exception as error:
            self.reader.set_exception(error)
        finally:
            self.reader.feed_eof()

    def write(self, data):
        self.pending.extend(data)

    async def drain(self):
        if self.pending:
            data = bytes(self.pending)
            self.pending.clear()
            await self.socket.send_bytes(data)

    def close(self):
        if self.closed is None:
            self.closed = asyncio.create_task(self.cleanup())

    async def cleanup(self):
        await self.socket.close()
        self.receiving.cancel()
        await asyncio.gather(self.receiving, return_exceptions=True)
        await self.session.close()

    async def wait_closed(self):
        if self.closed is not None:
            await self.closed


class ConnectionWebsocket(ConnectionTcpAbridged):
    async def _connect(self, timeout=None, ssl=None):
        stamp, nonce = int(time.time()), secrets.token_hex(16)
        payload = f"{self._ip}:{self._port}:{stamp}:{nonce}"
        key = bytes.fromhex(os.environ["IPA_OBJECT_KEY"])
        signature = hmac.new(key, payload.encode(), hashlib.sha256).hexdigest()
        session = aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=None, sock_connect=10))
        try:
            socket = await session.ws_connect(
                os.environ["TG_TRANSPORT_URL"],
                params={"ip": self._ip, "port": self._port, "time": stamp, "nonce": nonce},
                headers={"X-Signum-Signature": signature}, heartbeat=30,
                max_msg_size=2 * 1024 * 1024,
            )
            self._reader = asyncio.StreamReader()
            self._writer = WebsocketWriter(session, socket, self._reader)
            self._codec = self.packet_codec(self)
            self._init_conn()
            await self._writer.drain()
        except BaseException:
            await session.close()
            raise
