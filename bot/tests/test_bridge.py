from __future__ import annotations

import base64
import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock, patch

from signum_bot.bridge import BridgeWorker, seal, unseal, upload_https


class FileTransferTests(unittest.TestCase):
    def test_chunked_upload_preserves_offsets_and_checks_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "test.ipa"
            content = b"x" * (512 * 1024 + 17)
            path.write_bytes(content)
            calls = []

            def exchange(http, operation, **fields):
                calls.append((operation, fields))
                return {
                    "size": fields.get("offset", 0)
                    + len(base64.b64decode(fields.get("data", "")))
                }

            with (
                patch("signum_bot.bridge.httpx.Client"),
                patch("signum_bot.bridge.file_exchange", side_effect=exchange),
            ):
                upload_https(
                    "token", path, json.dumps({"data": "meta"}).encode(), len(content)
                )
            self.assertEqual(
                [op for op, _ in calls],
                ["file_begin", "file_chunk", "file_chunk", "file_finish"],
            )
            chunks = calls[1:3]
            self.assertEqual(
                [fields["offset"] for _, fields in chunks], [0, 512 * 1024]
            )
            self.assertEqual(
                b"".join(base64.b64decode(fields["data"]) for _, fields in chunks),
                content,
            )
            self.assertEqual(
                calls[-1][1]["sha256"], hashlib.sha256(content).hexdigest()
            )

    def test_failed_upload_revokes_partial_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "test.ipa"
            path.write_bytes(b"content")
            exchange = MagicMock(side_effect=[{}, RuntimeError("transfer failed"), {}])
            with (
                patch("signum_bot.bridge.httpx.Client"),
                patch("signum_bot.bridge.file_exchange", exchange),
                self.assertRaises(RuntimeError),
            ):
                upload_https("token", path, b"{}", 7)
            self.assertEqual(exchange.call_args.args[1], "file_delete")


class BridgeTests(unittest.IsolatedAsyncioTestCase):
    async def test_requests_without_headers_reach_api_and_return_auth_error(self):
        cfg = SimpleNamespace(public_base_url="https://example.com", http_port=8088)
        key = bytes(range(32))
        with patch("signum_bot.bridge.bridge_key", return_value=key):
            worker = BridgeWorker(cfg)
        worker.exchange = AsyncMock()
        http = SimpleNamespace(
            request=AsyncMock(
                return_value=SimpleNamespace(
                    content=b'{"error":"invalid_api_key"}',
                    status_code=401,
                    headers={"content-type": "application/json"},
                )
            )
        )
        for headers in ({}, []):
            with (
                self.subTest(headers=headers),
                patch("signum_bot.bridge.time.time", return_value=100),
            ):
                payload = seal(
                    {
                        "path": "/api/v1/me",
                        "method": "GET",
                        "headers": headers,
                        "ip": "203.0.113.10",
                        "body": "",
                        "deadline": 124,
                    },
                    key,
                )
                await worker.deliver(http, {"id": "test", "payload": payload})
                self.assertEqual(
                    http.request.await_args.args[:2],
                    ("GET", "http://127.0.0.1:8088/api/v1/me"),
                )
                reply = unseal(worker.exchange.await_args.kwargs["payload"], key)
                self.assertEqual(reply["status"], 401)
                self.assertEqual(
                    base64.b64decode(reply["body"]), b'{"error":"invalid_api_key"}'
                )

    async def test_external_route_is_rejected_before_forwarding(self):
        cfg = SimpleNamespace(public_base_url="https://example.com", http_port=8088)
        key = bytes(range(32))
        with patch("signum_bot.bridge.bridge_key", return_value=key):
            worker = BridgeWorker(cfg)
        worker.exchange = AsyncMock()
        http = SimpleNamespace(request=AsyncMock())
        payload = seal({"path": "https://outside.example/api/v1/me"}, key)
        await worker.deliver(http, {"id": "test", "payload": payload})
        http.request.assert_not_awaited()
        self.assertEqual(
            unseal(worker.exchange.await_args.kwargs["payload"], key)["status"], 503
        )
