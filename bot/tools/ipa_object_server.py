from __future__ import annotations

import asyncio
import base64
import hashlib
import hmac
import json
import os
import re
import shutil
import time
import ipaddress
from pathlib import Path

from aiohttp import web
from cachetools import TTLCache

TOKEN = re.compile(r"[A-Za-z0-9_-]{16,64}")
MAX_SIZE = 4096 * 1024 * 1024


def create_app(folder: Path, key: bytes):
    folder.mkdir(parents=True, exist_ok=True, mode=0o700)
    nonces = TTLCache(maxsize=4096, ttl=120)
    slots = asyncio.Semaphore(16)
    read_slots = asyncio.Semaphore(8)
    locks = {}
    transports = set()

    async def telegram_transport(request):
        if len(transports) >= 12:
            raise web.HTTPTooManyRequests()
        try:
            ip = ipaddress.ip_address(request.query['ip'])
            port = int(request.query['port'])
            stamp = int(request.query['time'])
            nonce = request.query['nonce']
            networks = ('149.154.160.0/20', '91.108.4.0/22', '91.108.8.0/22',
                        '91.108.12.0/22', '91.108.16.0/22', '91.108.56.0/22')
            if port not in (80, 443) or not any(ip in ipaddress.ip_network(n) for n in networks):
                raise ValueError()
            if abs(time.time() - stamp) > 60 or not re.fullmatch(r'[a-f0-9]{32}', nonce) or nonce in nonces:
                raise ValueError()
            payload = f'{ip}:{port}:{stamp}:{nonce}'
            signature = hmac.new(key, payload.encode(), hashlib.sha256).hexdigest()
            if not hmac.compare_digest(signature, request.headers.get('X-Signum-Signature', '')):
                raise ValueError()
            nonces[nonce] = True
        except (KeyError, ValueError):
            raise web.HTTPUnauthorized() from None
        lease = object()
        transports.add(lease)
        socket = web.WebSocketResponse(heartbeat=30, max_msg_size=2 * 1024 * 1024)
        writer = None
        tasks = []
        try:
            reader, writer = await asyncio.wait_for(asyncio.open_connection(str(ip), port), timeout=10)
            await socket.prepare(request)
            async def incoming():
                async for message in socket:
                    if message.type == web.WSMsgType.BINARY:
                        writer.write(message.data)
                        await writer.drain()
            async def outgoing():
                while chunk := await reader.read(65536):
                    await socket.send_bytes(chunk)
            tasks = [asyncio.create_task(incoming()), asyncio.create_task(outgoing())]
            await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
            return socket
        finally:
            transports.discard(lease)
            for task in tasks:
                task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            if writer:
                writer.close()
                await asyncio.gather(writer.wait_closed(), return_exceptions=True)
            await asyncio.gather(socket.close(), return_exceptions=True)

    async def operation(request):
        async with read_slots:
            body = await request.read()
            signature = hmac.new(key, body, hashlib.sha256).hexdigest()
            if not hmac.compare_digest(signature, request.headers.get("X-Signum-Signature", "")):
                raise web.HTTPUnauthorized()
        try:
            data = json.loads(body)
            nonce = data["nonce"]
            if (not isinstance(nonce, str) or not re.fullmatch(r"[a-f0-9]{32}", nonce)
                    or abs(int(data["time"]) - time.time()) > 60 or nonce in nonces):
                raise ValueError("Invalid request")
            nonces[nonce] = True
            token = data["token"]
            if not isinstance(token, str) or not TOKEN.fullmatch(token):
                raise ValueError("Invalid token")
        except (KeyError, ValueError, TypeError):
            raise web.HTTPBadRequest() from None
        async with slots:
            lock, users = locks.get(token, (asyncio.Lock(), 0))
            locks[token] = lock, users + 1
            try:
                async with lock:
                    return await handle(data, token)
            finally:
                lock, users = locks[token]
                if users == 1:
                    del locks[token]
                else:
                    locks[token] = lock, users - 1

    async def handle(data, token):
        part = folder / (token + ".part")
        final = folder / (token + ".ipa")
        record = folder / (token + ".json")
        operation = data.get("op")
        if operation == "file_delete":
            for suffix in (".ipa", ".part", ".json", ".pending"):
                (folder / (token + suffix)).unlink(missing_ok=True)
            return web.json_response({"deleted": True})
        if operation == "file_status":
            return web.json_response({"exists": final.is_file()})
        if operation == "file_begin":
            size = data.get("size")
            if type(size) is not int or not 0 < size <= MAX_SIZE:
                raise web.HTTPBadRequest()
            if final.exists() or part.exists():
                raise web.HTTPConflict()
            if len(list(folder.glob("*.part"))) >= 12 or shutil.disk_usage(folder).free < size + 1024**3:
                raise web.HTTPInsufficientStorage()
            part.touch(mode=0o600)
            (folder / (token + ".pending")).write_text(json.dumps({"size": size, "created": int(time.time())}))
            return web.json_response({"size": 0})
        if not part.is_file():
            raise web.HTTPNotFound()
        pending = json.loads((folder / (token + ".pending")).read_text())
        if operation == "file_chunk":
            try:
                chunk = base64.b64decode(data["data"], validate=True)
                offset = data["offset"]
                if type(offset) is not int or not chunk or len(chunk) > 2 * 1024 * 1024:
                    raise ValueError()
            except (ValueError, TypeError, KeyError):
                raise web.HTTPBadRequest() from None
            if offset != part.stat().st_size:
                raise web.HTTPConflict()
            if offset + len(chunk) > pending["size"]:
                raise web.HTTPRequestEntityTooLarge(max_size=pending["size"], actual_size=offset + len(chunk))
            with part.open("ab") as stream:
                stream.write(chunk)
            return web.json_response({"size": offset + len(chunk)})
        if operation != "file_finish":
            raise web.HTTPBadRequest()
        try:
            metadata = json.loads(base64.b64decode(data["metadata"]["data"], validate=True))
            expires = metadata["expires_at"]
            if type(expires) is not int or not time.time() < expires <= time.time() + 13 * 3600:
                raise ValueError()
        except (KeyError, ValueError, TypeError):
            raise web.HTTPBadRequest() from None
        def checksum():
            with part.open("rb") as stream:
                return hashlib.file_digest(stream, "sha256").hexdigest()
        if part.stat().st_size != pending["size"] or not hmac.compare_digest(await asyncio.to_thread(checksum), str(data.get("sha256", ""))):
            raise web.HTTPBadRequest()
        record.write_text(json.dumps({"size": pending["size"], "expires_at": expires}))
        record.chmod(0o600)
        os.replace(part, final)
        (folder / (token + ".pending")).unlink()
        return web.json_response({"stored": True})

    async def download(request):
        token = request.match_info["token"]
        try:
            expires = int(request.query["expires"])
            if not TOKEN.fullmatch(token) or not time.time() < expires <= time.time() + 300:
                raise ValueError()
            signature = hmac.new(key, f"{token}:{expires}".encode(), hashlib.sha256).hexdigest()
            if not hmac.compare_digest(signature, request.query.get("signature", "")):
                raise ValueError()
            record = json.loads((folder / (token + ".json")).read_text())
            if record["expires_at"] <= time.time() or not (folder / (token + ".ipa")).is_file():
                raise ValueError()
        except (KeyError, ValueError, OSError):
            raise web.HTTPNotFound() from None
        return web.FileResponse(folder / (token + ".ipa"), headers={"Cache-Control": "no-store"})

    async def cleaner(app):
        async def loop():
            while True:
                for file in folder.glob("*.json"):
                    try:
                        expired = json.loads(file.read_text())["expires_at"] <= time.time()
                    except (ValueError, KeyError):
                        expired = True
                    if expired:
                        for suffix in (".json", ".ipa"):
                            file.with_suffix(suffix).unlink(missing_ok=True)
                for file in folder.glob("*.pending"):
                    if file.stat().st_mtime < time.time() - 3600:
                        file.with_suffix(".part").unlink(missing_ok=True)
                        file.unlink(missing_ok=True)
                await asyncio.sleep(60)
        task = asyncio.create_task(loop())
        yield
        task.cancel()
        await asyncio.gather(task, return_exceptions=True)

    app = web.Application(client_max_size=3 * 1024 * 1024)
    app.router.add_post("/signum-objects/_bridge", operation)
    app.router.add_get("/signum-objects/f/{token}.ipa", download)
    app.router.add_get("/signum-transport", telegram_transport)
    app.cleanup_ctx.append(cleaner)
    return app


if __name__ == "__main__":
    web.run_app(create_app(Path(os.environ["IPA_OBJECT_DIR"]), bytes.fromhex(os.environ["IPA_OBJECT_KEY"])),
                host="127.0.0.1", port=8097, access_log=None)
