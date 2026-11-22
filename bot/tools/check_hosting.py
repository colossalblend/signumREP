from __future__ import annotations

import ftplib
import os
import plistlib
import secrets
import sys
import tempfile
import time
from pathlib import Path

import httpx

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from signum_bot.bridge import connect_ftp, delete_remote, file_exchange, upload_file  # noqa: E402
from signum_bot.config import Config  # noqa: E402


def main():
    cfg = Config.load(require_bot_token=False)
    token = secrets.token_urlsafe(18)
    with (
        tempfile.TemporaryDirectory() as directory,
        httpx.Client(timeout=httpx.Timeout(30, connect=5), trust_env=False) as http,
    ):
        path = Path(directory) / "hosting-check.ipa"
        content = secrets.token_bytes(1048579)
        path.write_bytes(content)
        row = dict(
            token=token,
            path=str(path),
            title="Signum hosting check",
            name=path.name,
            size=len(content),
            bundle_id="pw.appbank.signum.check",
            version="1.0",
            expires_at=int(time.time()) + 300,
            ttl_hours=1,
        )
        try:
            upload_file(cfg, row)
            base = cfg.public_base_url
            response = http.get(f"{base}/f/{token}.ipa")
            assert response.status_code == 200 and response.content == content
            print("file_content: ok", flush=True)
            response = http.head(f"{base}/f/{token}.ipa")
            assert response.status_code == 200
            assert int(response.headers["content-length"]) == len(content)
            for value, expected in (
                ("bytes=16-31", content[16:32]),
                ("bytes=-32", content[-32:]),
            ):
                response = http.get(f"{base}/f/{token}.ipa", headers={"Range": value})
                assert response.status_code == 206 and response.content == expected
            assert (
                http.get(
                    f"{base}/f/{token}.ipa",
                    headers={"Range": f"bytes={len(content) + 1}-"},
                ).status_code
                == 416
            )
            print("head_and_range: ok", flush=True)
            page = http.get(f"{base}/i/{token}")
            assert page.status_code == 200 and "itms-services://" in page.text
            manifest = http.get(f"{base}/ota/{token}.plist")
            data = plistlib.loads(manifest.content)
            assert data["items"][0]["assets"][0]["url"] == f"{base}/f/{token}.ipa"
            print("installation_page_and_manifest: ok", flush=True)
            assert http.post(base + "/_bridge", content=b"{}").status_code == 403
            for route in ("/signum-private/config.php", "/docs.json", "/panel.json"):
                assert http.get(base + route).status_code in (403, 404)
            print("private_storage_and_unsigned_requests: blocked", flush=True)
            delete_remote(token)
            row["expires_at"] = int(time.time()) - 1
            upload_file(cfg, row)
            for route in (f"/f/{token}.ipa", f"/i/{token}", f"/ota/{token}.plist"):
                assert http.get(base + route).status_code == 404
            print("expired_links: blocked", flush=True)
            deadline = time.monotonic() + 75
            while time.monotonic() < deadline:
                if os.environ.get("BRIDGE_FILE_TRANSPORT") == "https":
                    if not file_exchange(http, "file_status", token=token)["exists"]:
                        print("expired_file_auto_deleted: ok", flush=True)
                        return
                    time.sleep(5)
                    continue
                client = connect_ftp()
                try:
                    client.voidcmd("TYPE I")
                    root = os.environ.get("BRIDGE_STORAGE_DIR", "/signum-private")
                    try:
                        client.size(root + "/files/" + token + ".ipa")
                    except ftplib.error_perm as exc:
                        if not str(exc).startswith("550"):
                            raise
                        print("expired_file_auto_deleted: ok", flush=True)
                        return
                finally:
                    client.close()
                time.sleep(5)
            raise RuntimeError("Expired file was not automatically deleted")
        finally:
            delete_remote(token)


if __name__ == "__main__":
    main()
