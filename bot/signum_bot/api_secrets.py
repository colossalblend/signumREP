from __future__ import annotations

import hashlib
import hmac
import json
import os
from pathlib import Path

from cryptography.fernet import Fernet


class ApiSecrets:
    def __init__(self, data_dir):
        path = Path(os.environ["SIGNUM_SESSION_KEY_FILE"]) if os.environ.get("SIGNUM_SESSION_KEY_FILE") else data_dir / "secrets" / "apple-api.key"
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.exists():
            temporary = path.with_name("apple-api-" + os.urandom(8).hex() + ".tmp")
            with open(temporary, "xb") as stream:
                os.chmod(temporary, 0o600)
                stream.write(Fernet.generate_key())
                stream.flush()
                os.fsync(stream.fileno())
            try:
                os.link(temporary, path)
            except FileExistsError:
                pass
            finally:
                temporary.unlink()
        self.key = path.read_bytes()
        self.cipher = Fernet(self.key)

    def encrypt(self, value):
        return self.cipher.encrypt(value.encode()).decode()

    def decrypt(self, value):
        return self.cipher.decrypt(value.encode()).decode()

    def fingerprint(self, *values):
        return hmac.new(
            self.key, json.dumps(values, ensure_ascii=False).encode(), hashlib.sha256
        ).hexdigest()
