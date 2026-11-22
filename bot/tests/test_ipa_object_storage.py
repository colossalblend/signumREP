from __future__ import annotations

import base64
import hashlib
import hmac
import importlib.util
import json
import secrets
import tempfile
import time
import unittest
from pathlib import Path

from aiohttp.test_utils import TestClient, TestServer

spec = importlib.util.spec_from_file_location("ipa_object_server", Path(__file__).resolve().parents[1] / "tools/ipa_object_server.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ObjectStorageTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.folder = Path(self.temporary.name)
        self.key = secrets.token_bytes(32)
        self.token = secrets.token_urlsafe(24)
        self.client = TestClient(TestServer(module.create_app(self.folder, self.key)))
        await self.client.start_server()

    async def asyncTearDown(self):
        await self.client.close()
        self.temporary.cleanup()

    def signed(self, operation, **fields):
        body = json.dumps(dict(op=operation, token=self.token, time=int(time.time()), nonce=secrets.token_hex(16), **fields)).encode()
        return body, {"X-Signum-Signature": hmac.new(self.key, body, hashlib.sha256).hexdigest()}

    async def call(self, operation, **fields):
        body, headers = self.signed(operation, **fields)
        return await self.client.post("/signum-objects/_bridge", data=body, headers=headers)

    async def test_unsigned_and_replayed_requests_are_rejected(self):
        body, headers = self.signed("file_begin", size=8)
        self.assertEqual((await self.client.post("/signum-objects/_bridge", data=body)).status, 401)
        self.assertFalse(list(self.folder.iterdir()))
        self.assertEqual((await self.client.post("/signum-objects/_bridge", data=body, headers=headers)).status, 200)
        self.assertEqual((await self.client.post("/signum-objects/_bridge", data=body, headers=headers)).status, 400)

    async def test_transport_rejects_unsigned_and_private_destinations(self):
        for ip, port, signed in [('149.154.167.50', 443, False), ('127.0.0.1', 443, True), ('149.154.167.50', 22, True)]:
            stamp = int(time.time())
            nonce = secrets.token_hex(16)
            payload = f'{ip}:{port}:{stamp}:{nonce}'
            headers = {'X-Signum-Signature': hmac.new(self.key, payload.encode(), hashlib.sha256).hexdigest()} if signed else {}
            response = await self.client.get('/signum-transport', params={'ip': ip, 'port': str(port), 'time': str(stamp), 'nonce': nonce}, headers=headers)
            self.assertEqual(response.status, 401)

    async def test_upload_integrity_signed_ranges_and_revoke(self):
        content = b"private-ipa-fixture"
        self.assertEqual((await self.call("file_begin", size=len(content))).status, 200)
        self.assertEqual((await self.call("file_chunk", offset=1, data=base64.b64encode(content).decode())).status, 409)
        self.assertEqual((await self.call("file_chunk", offset=0, data=base64.b64encode(content).decode())).status, 200)
        metadata = {"data": base64.b64encode(json.dumps({"expires_at": int(time.time()) + 300, "email": "discard@example.com"}).encode()).decode()}
        self.assertEqual((await self.call("file_finish", metadata=metadata, sha256="bad")).status, 400)
        self.assertEqual((await self.call("file_finish", metadata=metadata, sha256=hashlib.sha256(content).hexdigest())).status, 200)
        self.assertNotIn("email", (self.folder / (self.token + ".json")).read_text())
        route = "/signum-objects/f/" + self.token + ".ipa"
        self.assertEqual((await self.client.get(route)).status, 404)
        expires = int(time.time()) + 100
        signature = hmac.new(self.key, f"{self.token}:{expires}".encode(), hashlib.sha256).hexdigest()
        url = route + f"?expires={expires}&signature={signature}"
        response = await self.client.get(url, headers={"Range": "bytes=2-5"})
        self.assertEqual(response.status, 206)
        self.assertEqual(await response.read(), content[2:6])
        self.assertEqual((await self.call("file_delete")).status, 200)
        self.assertEqual((await self.client.get(url)).status, 404)

    async def test_path_escape_and_excessive_size_are_rejected(self):
        original = self.token
        self.token = "../" + original
        self.assertEqual((await self.call("file_begin", size=8)).status, 400)
        self.token = original
        self.assertEqual((await self.call("file_begin", size=module.MAX_SIZE + 1)).status, 400)
        self.assertFalse(list(self.folder.iterdir()))
