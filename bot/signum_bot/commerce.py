from __future__ import annotations

import hashlib
import secrets

from .config import Config
from .db import PLAN_DAYS, Db, now

API_TOPUPS = {"api100": 100, "api500": 500, "api1000": 1000}


def create_order(
    cfg: Config, db: Db, tg_id: int, plan: str, request_key: str = ""
) -> str:
    if plan in API_TOPUPS:
        user = db.one("select api_until from users where tg_id=?", (tg_id,))
        if not user or user["api_until"] <= now():
            raise ValueError("api_subscription_required")
    identifier = secrets.token_urlsafe(16)
    db._run(
        "insert into orders (id,tg_id,plan,amount,units,mode,created_at,expires_at,request_key) values (?,?,?,?,?,?,?,?,?)",
        (
            identifier,
            tg_id,
            plan,
            cfg.amount(plan),
            cfg.api_downloads if plan == "api" else API_TOPUPS.get(plan, 0),
            cfg.payment_mode,
            now(),
            now() + 3600,
            request_key,
        ),
    )
    return identifier


def activate_demo(db: Db, tg_id: int, identifier: str) -> str:
    order = db.one("select * from orders where id=? and tg_id=?", (identifier, tg_id))
    if not order or order["mode"] != "demo" or db.blocked(tg_id):
        return "invalid"
    if order["status"] == "complete":
        return "complete"
    if order["expires_at"] <= now():
        return "expired"
    return fulfill_order(db, order, "demo", "demo:" + identifier)


def fulfill_order(db: Db, order, mode: str, charge_id: str, *, notify_admins=()) -> str:
    if order["plan"] not in {*PLAN_DAYS, "single", "api", *API_TOPUPS}:
        return "invalid"
    with db.conn:
        claimed = db.conn.execute(
            "update orders set status='complete' where id=? and status='pending'",
            (order["id"],),
        )
        if claimed.rowcount != 1:
            return "complete"
        plan = order["plan"]
        tg_id = order["tg_id"]
        moment = now()
        if plan == "single":
            db.conn.execute(
                "update users set credits=credits+1 where tg_id=?", (tg_id,)
            )
        elif plan == "api":
            db.conn.execute(
                "update users set api_remaining=(case when api_until>? then api_remaining else 0 end)+?, api_until=max(api_until,?)+? where tg_id=?",
                (
                    moment,
                    order["units"],
                    moment,
                    30 * 86400,
                    tg_id,
                ),
            )
        elif plan in API_TOPUPS:
            db.conn.execute(
                "update users set api_extra_remaining=api_extra_remaining+? where tg_id=?",
                (order["units"], tg_id),
            )
        else:
            db.conn.execute(
                "update users set sub_until=max(sub_until,?)+? where tg_id=?",
                (moment, PLAN_DAYS[plan] * 86400, tg_id),
            )
        db.conn.execute(
            "insert into payments(tg_id,plan,amount,currency,charge_id,created_at,mode) values (?,?,?,'RUB',?,?,?)",
            (tg_id, plan, order["amount"], charge_id, moment, mode),
        )
        if mode == "real":
            db.conn.executemany(
                "insert or ignore into payment_notifications(charge_id,admin_id,created_at) values (?,?,?)",
                [(charge_id, admin_id, moment) for admin_id in set(notify_admins)],
            )
    return "activated"


def key_digest(key: str) -> str:
    return hashlib.sha256(key.encode()).hexdigest()


def upgrade_api_packages(cfg: Config, db: Db) -> None:
    orders = db.all(
        "select o.* from orders o join users u on u.tg_id=o.tg_id where o.plan='api' and o.units<? and (o.status='pending' or (o.status='complete' and u.api_until>?))",
        (cfg.api_downloads, now()),
    )
    with db.conn:
        for order in orders:
            if order["status"] == "complete":
                db.conn.execute(
                    "update users set api_remaining=api_remaining+? where tg_id=?",
                    (
                        max(0, cfg.api_downloads - order["units"]),
                        order["tg_id"],
                    ),
                )
            db.conn.execute(
                "update orders set units=? where id=?",
                (cfg.api_downloads, order["id"]),
            )


def issue_api_key(
    db: Db,
    tg_id: int,
    *,
    scopes: str = "read downloads billing accounts keys",
    days: int = 30,
) -> str:
    key = "sg_" + secrets.token_urlsafe(32)
    with db.conn:
        db.conn.execute("delete from api_keys where tg_id=? and is_default=1", (tg_id,))
        db.conn.execute(
            "insert into api_keys(digest,tg_id,created_at,expires_at,scopes,id,is_default) values (?,?,?,?,?,?,1)",
            (
                key_digest(key),
                tg_id,
                now(),
                now() + days * 86400,
                scopes,
                secrets.token_hex(8),
            ),
        )
    return key
