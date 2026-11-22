from __future__ import annotations

import secrets

from .commerce import key_digest
from .db import now

SCOPES = frozenset({"read", "downloads", "accounts", "billing", "keys"})
FULL_SCOPES = "read downloads accounts billing keys"
KEY_FIELDS = "id,tg_id,label,state,created_at,expires_at,last_used,scopes,quota_remaining,is_default"


def audit(db, actor, tg_id, action, target=""):
    db.conn.execute(
        "insert into api_audit(actor,tg_id,action,target,created_at) values (?,?,?,?,?)",
        (actor, tg_id, action, target, now()),
    )


def create_key(
    db,
    tg_id,
    label="Основной",
    *,
    scopes=FULL_SCOPES,
    expires_at=None,
    quota=None,
    actor=None,
):
    if not isinstance(label, str) or not 1 <= len(label.strip()) <= 40:
        raise ValueError("invalid_key_name")
    rights = frozenset(scopes.split()) if isinstance(scopes, str) else frozenset(scopes)
    if not rights <= SCOPES or "read" not in rights:
        raise ValueError("invalid_scopes")
    if quota is not None and (type(quota) is not int or not 0 <= quota <= 1_000_000):
        raise ValueError("invalid_quota")
    if type(expires_at) is not int or not now() < expires_at <= now() + 365 * 86400:
        raise ValueError("invalid_expiry")
    if (
        db.one(
            "select count(*) from api_keys where tg_id=? and state!='revoked'", (tg_id,)
        )[0]
        >= 20
    ):
        raise ValueError("too_many_keys")
    identifier, secret = secrets.token_hex(8), "sg_" + secrets.token_urlsafe(32)
    with db.conn:
        db.conn.execute(
            "insert into api_keys(digest,tg_id,created_at,id,label,expires_at,scopes,quota_remaining) values (?,?,?,?,?,?,?,?)",
            (
                key_digest(secret),
                tg_id,
                now(),
                identifier,
                label.strip(),
                expires_at,
                " ".join(sorted(rights)),
                quota,
            ),
        )
        audit(
            db, actor if actor is not None else tg_id, tg_id, "key_created", identifier
        )
    return identifier, secret


def keys_for(db, tg_id):
    return db.all(
        f"select {KEY_FIELDS} from api_keys where tg_id=? order by created_at desc,id",
        (tg_id,),
    )


def update_key(db, tg_id, identifier, changes, *, actor=None):
    key = db.one("select * from api_keys where id=? and tg_id=?", (identifier, tg_id))
    if not key:
        raise ValueError("key_not_found")
    if key["state"] == "revoked":
        raise ValueError("key_revoked")
    allowed = {"state", "label", "quota_remaining", "expires_at"}
    if not changes or not changes.keys() <= allowed:
        raise ValueError("invalid_key_update")
    for field, value in changes.items():
        if field == "state" and (
            not isinstance(value, str) or value not in {"active", "paused", "revoked"}
        ):
            raise ValueError("invalid_key_state")
        if field == "label" and (
            not isinstance(value, str) or not 1 <= len(value.strip()) <= 40
        ):
            raise ValueError("invalid_key_name")
        if (
            field == "quota_remaining"
            and value is not None
            and (type(value) is not int or not 0 <= value <= 1_000_000)
        ):
            raise ValueError("invalid_quota")
        if field == "expires_at" and (
            type(value) is not int or not now() < value <= now() + 365 * 86400
        ):
            raise ValueError("invalid_expiry")
    with db.conn:
        assignments = ",".join(f"{field}=?" for field in changes)
        db.conn.execute(
            f"update api_keys set {assignments} where id=? and tg_id=?",
            (*changes.values(), identifier, tg_id),
        )
        audit(
            db,
            actor if actor is not None else tg_id,
            tg_id,
            "key_updated:" + ",".join(changes),
            identifier,
        )
    return db.one(f"select {KEY_FIELDS} from api_keys where id=?", (identifier,))
