from __future__ import annotations

import asyncio
import base64
import ftplib
import hashlib
import hmac
import io
import json
import logging
import os
import re
import secrets
import ssl
import time
from pathlib import Path
from urllib.parse import urlsplit

import httpx
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from . import ota

LOG = logging.getLogger("signum-bot.bridge")
UPLOAD_SLOTS = asyncio.Semaphore(2)


def bridge_key() -> bytes:
    key = bytes.fromhex(os.environ["BRIDGE_KEY"])
    if len(key) != 32:
        raise ValueError("Invalid bridge key")
    return key


def seal(payload, key: bytes) -> str:
    nonce = secrets.token_bytes(12)
    raw = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode()
    return base64.b64encode(nonce + AESGCM(key).encrypt(nonce, raw, None)).decode()


def unseal(payload: str, key: bytes):
    raw = base64.b64decode(payload, validate=True)
    return json.loads(AESGCM(key).decrypt(raw[:12], raw[12:], None))


def connect_ftp():
    context = ssl.create_default_context()
    if chain := os.environ.get("BRIDGE_FTPS_CA_FILE"):
        context.load_verify_locations(cafile=chain)
    client = ftplib.FTP_TLS(context=context, timeout=900)
    try:
        client.connect(os.environ["BRIDGE_FTP_HOST"], timeout=30)
        client.host = os.environ.get("BRIDGE_FTPS_TLS_NAME", "hosting.reg.ru")
        client.login(os.environ["BRIDGE_FTP_USER"], os.environ["BRIDGE_FTP_PASSWORD"])
        client.prot_p()
        client.sock.settimeout(900)
        return client
    except BaseException:
        client.close()
        raise


def checked_token(token):
    if not re.fullmatch(r"[A-Za-z0-9_-]{16,64}", token):
        raise ValueError("Invalid file identifier")
    return token


def file_exchange(http, operation, *, object_store=False, **fields):
    body = json.dumps(
        {
            "op": operation,
            "time": int(time.time()),
            "nonce": secrets.token_hex(16),
            **fields,
        },
        separators=(",", ":"),
    ).encode()
    response = http.post(
        (os.environ["IPA_OBJECT_URL"] if object_store else os.environ["PUBLIC_BASE_URL"]).rstrip("/") + "/_bridge",
        content=body,
        headers={
            "Content-Type": "application/json",
            "X-Signum-Signature": hmac.new(
                bytes.fromhex(os.environ["IPA_OBJECT_KEY"]) if object_store else bridge_key(), body, hashlib.sha256
            ).hexdigest(),
        },
    )
    response.raise_for_status()
    return response.json()


def upload_https(token, path, envelope, size):
    remote = bool(os.environ.get("IPA_OBJECT_URL"))
    with httpx.Client(timeout=httpx.Timeout(90, connect=5), trust_env=False) as http:
        file_exchange(http, "file_begin", object_store=remote, token=token, size=size)
        try:
            digest = hashlib.sha256()
            offset = 0
            with path.open("rb") as stream:
                while chunk := stream.read((2 * 1024 * 1024) if remote else (512 * 1024)):
                    digest.update(chunk)
                    result = file_exchange(
                        http,
                        "file_chunk",
                        object_store=remote,
                        token=token,
                        offset=offset,
                        data=base64.b64encode(chunk).decode(),
                    )
                    offset += len(chunk)
                    if result.get("size") != offset:
                        raise RuntimeError("Remote file size mismatch")
            file_exchange(
                http,
                "file_finish",
                object_store=remote,
                token=token,
                metadata=json.loads(envelope),
                sha256=digest.hexdigest(),
            )
            if remote:
                file_exchange(http, "file_register", token=token, metadata=json.loads(envelope))
        except BaseException:
            try:
                file_exchange(http, "file_delete", object_store=remote, token=token)
            except Exception:
                LOG.warning("Interrupted HTTPS upload cleanup deferred")
            raise


def upload_file(cfg, row):
    token = checked_token(row["token"])
    path = Path(row["path"])
    if path.stat().st_size != row["size"]:
        raise ValueError("File size mismatch")
    metadata = {
        name: row[name]
        for name in (
            "title",
            "name",
            "size",
            "bundle_id",
            "version",
            "expires_at",
            "ttl_hours",
        )
    }
    if os.environ.get("IPA_OBJECT_URL"):
        metadata["remote"] = True
    links = ota.installation_links(cfg.public_base_url, token)
    metadata["manifest"] = base64.b64encode(
        ota.manifest(
            f"{cfg.public_base_url}/f/{token}.ipa",
            row["bundle_id"],
            row["version"],
            row["title"],
        )
    ).decode()
    import html

    metadata["page"] = ota.PAGE.format(
        title=html.escape(row["title"]),
        artist="",
        version=html.escape(row["version"]),
        size=ota.human_size(row["size"]),
        icon="",
        manifest_url=html.escape(links["manifest_url"]),
        ttl=row["ttl_hours"],
    )
    encoded = json.dumps(metadata, ensure_ascii=False, separators=(",", ":")).encode()
    envelope = json.dumps(
        {
            "data": base64.b64encode(encoded).decode(),
            "mac": hmac.new(bridge_key(), encoded, hashlib.sha256).hexdigest(),
        }
    ).encode()
    if os.environ.get("BRIDGE_FILE_TRANSPORT") == "https":
        upload_https(token, path, envelope, row["size"])
        return
    root = os.environ.get("BRIDGE_STORAGE_DIR", "/signum-private") + "/files/" + token
    client = connect_ftp()
    try:
        with path.open("rb") as stream:
            client.storbinary("STOR " + root + ".part", stream, blocksize=256 * 1024)
        if client.size(root + ".part") != row["size"]:
            raise RuntimeError("Remote file size mismatch")
        client.rename(root + ".part", root + ".ipa")
        client.storbinary("STOR " + root + ".json.part", io.BytesIO(envelope))
        client.rename(root + ".json.part", root + ".json")
    except BaseException:
        for suffix in (".part", ".json.part", ".json", ".ipa"):
            try:
                client.delete(root + suffix)
            except ftplib.all_errors:
                pass
        raise
    finally:
        client.close()


async def publish_file(cfg, db, token):
    row = db.file(token)
    async with UPLOAD_SLOTS:
        # Await the upload even on cancellation: a thread cannot be stopped halfway.
        task = asyncio.create_task(asyncio.to_thread(upload_file, cfg, row))
        try:
            await asyncio.shield(task)
        except asyncio.CancelledError:
            await task
            await revoke_file(cfg, token)
            raise
    db._run("update files set remote=1 where token=?", (token,))


def delete_remote(token):
    token = checked_token(token)
    if os.environ.get("BRIDGE_FILE_TRANSPORT") == "https":
        with httpx.Client(
            timeout=httpx.Timeout(30, connect=5), trust_env=False
        ) as http:
            file_exchange(http, "file_delete", token=token)
            if os.environ.get("IPA_OBJECT_URL"):
                file_exchange(http, "file_delete", object_store=True, token=token)
        return
    client = connect_ftp()
    try:
        root = (
            os.environ.get("BRIDGE_STORAGE_DIR", "/signum-private") + "/files/" + token
        )
        for suffix in (".json", ".ipa", ".part", ".json.part"):
            try:
                client.delete(root + suffix)
            except ftplib.error_perm as exc:
                if not str(exc).startswith("550"):
                    raise
    finally:
        client.close()


async def revoke_file(cfg, token):
    async with UPLOAD_SLOTS:
        await asyncio.to_thread(delete_remote, token)


class BridgeWorker:
    def __init__(self, cfg, app=None):
        self.cfg = cfg
        self.key = bridge_key()
        self.active = set()
        self.url = cfg.public_base_url + "/_bridge"
        self.local = f"http://127.0.0.1:{cfg.http_port}"
        self.app = app

    async def request(self, http, method, path, *, content, headers, timeout):
        if self.app is not None:
            from .bridge_dispatch import dispatch
            async with asyncio.timeout(timeout):
                return await dispatch(self.app, method, path, content, headers)
        return await http.request(method, self.local + path, content=content,
                                  headers=headers, timeout=timeout)

    async def exchange(self, http, operation, **fields):
        envelope = {
            "op": operation,
            "time": int(time.time()),
            "nonce": secrets.token_hex(16),
            **fields,
        }
        body = json.dumps(envelope, separators=(",", ":")).encode()
        signature = hmac.new(self.key, body, hashlib.sha256).hexdigest()
        response = await http.post(
            self.url,
            content=body,
            headers={
                "Content-Type": "application/json",
                "X-Signum-Signature": signature,
            },
        )
        response.raise_for_status()
        return response.json()

    async def deliver(self, http, item):
        try:
            payload = unseal(item["payload"], self.key)
            target = urlsplit(payload["path"])
            if (
                target.scheme
                or target.netloc
                or not (
                    target.path.startswith("/api/v1/")
                    or target.path == "/payments/yookassa"
                )
            ):
                raise ValueError("Invalid bridge route")
            remaining = min(22, payload["deadline"] - time.time())
            if remaining <= 0:
                return
            incoming_headers = payload["headers"]
            if incoming_headers == []:
                incoming_headers = {}
            if not isinstance(incoming_headers, dict):
                raise ValueError("Invalid bridge headers")
            headers = {
                name: value
                for name, value in incoming_headers.items()
                if name.lower() in {"authorization", "content-type", "idempotency-key"}
            }
            headers.update({"X-Real-IP": payload["ip"], "X-Forwarded-Proto": "https"})
            response = await self.request(
                http,
                payload["method"],
                payload["path"],
                content=base64.b64decode(payload["body"]),
                headers=headers,
                timeout=remaining,
            )
            if len(response.content) > 1024 * 1024:
                raise ValueError("Bridge response too large")
            result = {
                "status": response.status_code,
                "body": base64.b64encode(response.content).decode(),
                "type": response.headers.get("content-type", "application/json"),
            }
        except asyncio.CancelledError:
            raise
        except Exception as exc:
            LOG.warning("Bridge request failed (%s)", type(exc).__name__)
            result = {
                "status": 503,
                "body": base64.b64encode(
                    b'{"error":"temporarily_unavailable"}'
                ).decode(),
                "type": "application/json",
            }
        try:
            await self.exchange(
                http, "reply", id=item["id"], payload=seal(result, self.key)
            )
        except Exception as exc:
            LOG.warning("Bridge response delivery failed (%s)", type(exc).__name__)

    async def deliver_event(self, http, item):
        try:
            event = unseal(item["payload"], self.key)
            if event.get("id") != item["id"]:
                raise ValueError("Invalid event identifier")
            event["time"] = int(time.time())
            body = json.dumps(event, separators=(",", ":")).encode()
            response = await self.request(
                http, "POST", "/internal/install-events",
                content=body,
                headers={
                    "Content-Type": "application/json",
                    "X-Signum-Signature": hmac.new(
                        self.key, body, hashlib.sha256
                    ).hexdigest(),
                },
                timeout=10,
            )
            response.raise_for_status()
            await self.exchange(http, "event_ack", id=item["id"])
        except Exception as exc:
            LOG.warning("Installation event delivery failed (%s)", type(exc).__name__)

    async def run(self):
        delay = 1
        async with httpx.AsyncClient(
            timeout=httpx.Timeout(25, connect=5),
            limits=httpx.Limits(max_connections=40),
            trust_env=False,
        ) as http:
            try:
                while True:
                    try:
                        data = await self.exchange(
                            http, "pull", capacity=max(0, 16 - len(self.active))
                        )
                        for item in data.get("requests", []):
                            task = asyncio.create_task(self.deliver(http, item))
                            self.active.add(task)
                            task.add_done_callback(self.active.discard)
                        for item in data.get("events", []):
                            task = asyncio.create_task(self.deliver_event(http, item))
                            self.active.add(task)
                            task.add_done_callback(self.active.discard)
                        delay = 1
                        await asyncio.sleep(0.5 if data.get("requests") else 1)
                    except asyncio.CancelledError:
                        raise
                    except Exception as exc:
                        LOG.warning("Bridge unavailable (%s)", type(exc).__name__)
                        await asyncio.sleep(delay)
                        delay = min(60, delay * 2)
            finally:
                for task in tuple(self.active):
                    task.cancel()
                await asyncio.gather(*self.active, return_exceptions=True)
