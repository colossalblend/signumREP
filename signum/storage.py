from __future__ import annotations

import json
import secrets
from datetime import datetime, timezone
from pathlib import Path

import keyring

from signum.config import ROOT_DIR
from signum.models import AuthSession


SERVICE = "Signum"
AUTH_KEY = "apple_auth_session"
IPATOOL_KEYCHAIN_KEY = "ipatool_keychain_passphrase"


class CredentialStore:
    def __init__(self, cache_path: Path | None = None) -> None:
        self.cache_path = cache_path or ROOT_DIR / ".signum-cache.json"

    def save_auth_session(self, email: str) -> None:
        payload = {"email": email, "created_at": datetime.now(timezone.utc).isoformat()}
        keyring.set_password(SERVICE, AUTH_KEY, json.dumps(payload))

    def get_auth_session(self) -> AuthSession | None:
        raw = keyring.get_password(SERVICE, AUTH_KEY)
        if not raw:
            return None
        try:
            payload = json.loads(raw)
            return AuthSession(email=payload["email"], created_at=datetime.fromisoformat(payload["created_at"]))
        except Exception:
            return None

    def get_or_create_ipatool_keychain_passphrase(self) -> str:
        existing = keyring.get_password(SERVICE, IPATOOL_KEYCHAIN_KEY)
        if existing:
            return existing
        passphrase = secrets.token_urlsafe(32)
        keyring.set_password(SERVICE, IPATOOL_KEYCHAIN_KEY, passphrase)
        return passphrase

    def remember_known_app_ids(self, app_ids: set[str]) -> None:
        state = self._load_cache()
        state["known_app_ids"] = sorted(app_ids)
        self._save_cache(state)

    def get_known_app_ids(self) -> set[str]:
        state = self._load_cache()
        return set(state.get("known_app_ids", []))

    def mark_purchased(self, bundle_id: str) -> None:
        state = self._load_cache()
        purchased = set(state.get("purchased_bundle_ids", []))
        purchased.add(bundle_id)
        state["purchased_bundle_ids"] = sorted(purchased)
        self._save_cache(state)

    def is_purchased(self, bundle_id: str) -> bool:
        state = self._load_cache()
        return bundle_id in set(state.get("purchased_bundle_ids", []))

    def _load_cache(self) -> dict:
        if not self.cache_path.exists():
            return {}
        try:
            return json.loads(self.cache_path.read_text(encoding="utf-8"))
        except Exception:
            return {}

    def _save_cache(self, state: dict) -> None:
        self.cache_path.write_text(json.dumps(state, indent=2, ensure_ascii=False), encoding="utf-8")
