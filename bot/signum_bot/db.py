from __future__ import annotations

import os
import re
import sqlite3
import time
from contextlib import closing
from pathlib import Path
from typing import Any
from .database_crypto import connect_database

SCHEMA = """
create table if not exists users (
    tg_id      integer primary key,
    username   text default '',
    name       text default '',
    credits    integer default 0,
    sub_until  integer default 0,
    blocked    integer default 0,
    created_at integer default 0
);
create table if not exists accounts (
    id         integer primary key autoincrement,
    tg_id      integer default 0,
    email      text collate nocase,
    password   text default '',
    keychain   text default '',
    status     text default 'new',
    note       text default '',
    uses       integer default 0,
    last_used  integer default 0,
    created_at integer default 0,
    unique(tg_id,email)
);
create table if not exists apps (
    adam_id    text primary key,
    bundle_id  text default '',
    title      text default '',
    artist     text default '',
    icon       text default '',
    version    text default '',
    min_ios    text default '',
    size       integer default 0,
    source     text default '',
    updated_at integer default 0
);
create table if not exists files (
    token      text primary key,
    adam_id    text default '',
    account_id integer default 0,
    tg_id      integer default 0,
    name       text default '',
    path       text default '',
    size       integer default 0,
    bundle_id  text default '',
    version    text default '',
    title      text default '',
    signed     integer default 0,
    encrypted  integer default 0,
    created_at integer default 0,
    expires_at integer default 0
);
create table if not exists history (
    id         integer primary key autoincrement,
    tg_id      integer,
    adam_id    text default '',
    title      text default '',
    kind       text default '',
    account_id integer default 0,
    status     text default '',
    paid       text default '',
    created_at integer default 0
);
create table if not exists payments (
    id         integer primary key autoincrement,
    tg_id      integer,
    plan       text default '',
    amount     integer default 0,
    currency   text default '',
    charge_id  text default '',
    created_at integer default 0
);
create table if not exists identities (
    email      text,
    tg_id      integer,
    first_seen integer default 0,
    last_seen  integer default 0,
    primary key (email, tg_id)
);
create table if not exists payment_notifications (
    charge_id text not null,
    admin_id integer not null,
    created_at integer not null,
    delivered_at integer default 0,
    next_attempt integer default 0,
    attempts integer default 0,
    primary key (charge_id, admin_id)
);
create index if not exists payment_notifications_due on payment_notifications(delivered_at, next_attempt);
create index if not exists history_user on history (tg_id, created_at);
create table if not exists ownership_cache (
    account_id integer,
    session_key text,
    adam_id text,
    payload text,
    expires_at integer,
    primary key (account_id, session_key, adam_id)
);
create index if not exists ownership_expiry on ownership_cache (expires_at);
create table if not exists download_claims (
    id text primary key, tg_id integer not null, paid text not null, expires_at integer not null
);
create table if not exists service_settings (
    name text primary key, value text not null
);
create table if not exists orders (
    id text primary key,
    tg_id integer not null,
    plan text not null,
    amount integer not null,
    units integer default 0,
    mode text not null,
    status text default 'pending',
    created_at integer not null,
    expires_at integer not null
);
create table if not exists api_keys (
    digest text primary key,
    tg_id integer not null,
    created_at integer not null,
    id text unique not null,
    label text default 'Основной',
    state text default 'active',
    expires_at integer default 0,
    scopes text default 'read downloads billing accounts keys',
    last_used integer default 0,
    quota_remaining integer,
    is_default integer default 0
);
create index if not exists api_keys_owner on api_keys(tg_id);
create table if not exists api_accounts (
    id text primary key,
    tg_id integer not null,
    engine_id integer unique not null,
    email text not null collate nocase,
    password_secret text default '',
    keychain text not null,
    status text default 'new',
    session_generation integer default 0,
    kbsync_warmed_at integer default 0,
    created_at integer not null,
    unique(tg_id,email)
);
create table if not exists api_auth_jobs (
    id text primary key,
    tg_id integer not null,
    account_id text not null,
    key_id text not null,
    request_key text not null,
    fingerprint text not null,
    status text default 'queued',
    error text default '',
    challenge_digest text default '',
    attempts integer default 0,
    created_at integer not null,
    expires_at integer not null,
    unique(tg_id,request_key)
);
create index if not exists api_auth_status on api_auth_jobs(status,tg_id);
create table if not exists api_audit (
    id integer primary key autoincrement,
    actor integer not null,
    tg_id integer not null,
    action text not null,
    target text default '',
    created_at integer not null
);
create table if not exists api_jobs (
    id text primary key,
    tg_id integer not null,
    request_key text not null,
    adam_id text not null,
    version_id text default '',
    kind text not null,
    status text default 'queued',
    file_token text default '',
    error text default '',
    reserved integer default 1,
    created_at integer not null,
    unique(tg_id, request_key)
);
create index if not exists api_jobs_status on api_jobs(status, tg_id);
create table if not exists apple_web_sessions (
    account_id integer primary key,
    generation integer not null,
    session_secret text default '',
    history_secret text default '',
    updated_at integer default 0
);
create table if not exists storage_reservations (
    id text primary key, bytes integer not null, expires_at integer not null
);
create table if not exists installation_feedback (
    token text primary key, tg_id integer not null, status text default 'pending',
    due_at integer not null, attempts integer default 0, reason text default '',
    created_at integer not null
);
create table if not exists installation_events (
    id text primary key, created_at integer not null
);
"""

WEEK = 7 * 24 * 3600
PLAN_DAYS = {"week": 7, "month": 30, "halfyear": 180, "year": 365}


def now() -> int:
    return int(time.time())


class Db:
    def __init__(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        self.conn = connect_database(path, check_same_thread=False)
        self.conn.execute("pragma journal_mode=WAL")
        self.conn.execute("pragma synchronous=NORMAL")
        self.conn.executescript(SCHEMA)
        self.conn.commit()
        self._migrate()
        # В базе лежат пароли от Apple ID: файл не должен читаться посторонними.
        try:
            os.chmod(path, 0o600)
        except OSError:
            pass

    def _migrate(self) -> None:
        """Мелкие догонялки схемы: база живёт дольше, чем один релиз."""
        columns = {row["name"] for row in self.all("pragma table_info(accounts)")}
        if "session_generation" not in columns:
            self._run(
                "alter table accounts add column session_generation integer default 0"
            )
        if "kbsync_warmed_at" not in columns:
            self._run(
                "alter table accounts add column kbsync_warmed_at integer default 0"
            )
        for field in ("country_code", "country_name"):
            if field not in columns:
                self._run(f"alter table accounts add column {field} text default ''")
        if "tg_id" not in columns:
            self._run("alter table accounts add column tg_id integer default 0")
        definition = self.one(
            "select sql from sqlite_master where type='table' and name='accounts'"
        )[0]
        if re.search(r"\bemail\s+text\s+unique\b", definition, re.I):
            updated = re.sub(
                r"\bemail\s+text\s+unique\b",
                "email text collate nocase",
                definition,
                flags=re.I,
            )
            updated = re.sub(
                r"^create table(?: if not exists)? accounts",
                "create table accounts_next",
                updated,
                flags=re.I,
            )
            updated = updated[: updated.rfind(")")] + ", unique(tg_id,email))"
            with self.conn:
                self.conn.execute(updated)
                self.conn.execute("insert into accounts_next select * from accounts")
                self.conn.execute("drop table accounts")
                self.conn.execute("alter table accounts_next rename to accounts")
        api_columns = {
            row["name"] for row in self.all("pragma table_info(api_accounts)")
        }
        if "key_id" not in api_columns:
            self._run("alter table api_accounts add column key_id text default ''")
            self._run(
                "update api_accounts set key_id=coalesce((select key_id from api_auth_jobs j where j.account_id=api_accounts.id order by created_at limit 1),'')"
            )
        columns = {row["name"] for row in self.all("pragma table_info(files)")}
        if "encrypted" not in columns:
            self._run("alter table files add column encrypted integer default 0")
        if "paid_with" not in columns:
            self._run("alter table files add column paid_with text default ''")
        if "ttl_hours" not in columns:
            self._run("alter table files add column ttl_hours integer default 0")
        if "remote" not in columns:
            self._run("alter table files add column remote integer default 0")
        columns = {row["name"] for row in self.all("pragma table_info(users)")}
        for name in (
            "api_until",
            "api_remaining",
            "api_bytes_remaining",
            "api_extra_remaining",
            "api_enabled",
            "api_only",
        ):
            if name not in columns:
                default = 1 if name == "api_enabled" else 0
                self._run(
                    f"alter table users add column {name} integer default {default}"
                )
        columns = {row["name"] for row in self.all("pragma table_info(api_keys)")}
        for name, declaration in (
            ("expires_at", "integer default 0"),
            ("scopes", "text default 'read downloads'"),
            ("last_used", "integer default 0"),
        ):
            if name not in columns:
                self._run(f"alter table api_keys add column {name} {declaration}")
        self._run(
            "update api_keys set expires_at=(select api_until from users where users.tg_id=api_keys.tg_id) where expires_at=0"
        )
        if "id" not in columns:
            self.conn.executescript("""
                create table api_keys_next (
                    digest text primary key, tg_id integer not null, created_at integer not null,
                    id text unique not null, label text default 'Основной', state text default 'active',
                    expires_at integer default 0, scopes text default 'read downloads billing accounts keys',
                    last_used integer default 0, quota_remaining integer, is_default integer default 0
                );
                insert into api_keys_next(digest,tg_id,created_at,id,expires_at,scopes,last_used,is_default)
                select digest,tg_id,created_at,lower(hex(randomblob(8))),expires_at,
                    case when scopes='read' then scopes else scopes||' accounts keys' end,last_used,1 from api_keys;
                drop table api_keys;
                alter table api_keys_next rename to api_keys;
                create index api_keys_owner on api_keys(tg_id);
            """)
            self.conn.commit()
        columns = {row["name"] for row in self.all("pragma table_info(orders)")}
        if "traffic_bytes" not in columns:
            self._run("alter table orders add column traffic_bytes integer default 0")
        for name, declaration in (
            ("payment_id", "text default ''"),
            ("confirmation_url", "text default ''"),
            ("payment_test", "integer default 0"),
            ("payment_checked_at", "integer default 0"),
            ("request_key", "text default ''"),
        ):
            if name not in columns:
                self._run(f"alter table orders add column {name} {declaration}")
        self._run(
            "create unique index if not exists order_payment_id on orders(payment_id) where payment_id!=''"
        )
        self._run(
            "create unique index if not exists order_request_key on orders(tg_id,request_key) where request_key!=''"
        )
        columns = {row["name"] for row in self.all("pragma table_info(api_jobs)")}
        if "quota_source" not in columns:
            self._run(
                "alter table api_jobs add column quota_source text default 'base'"
            )
        for name, declaration in (
            ("key_id", "text default ''"),
            ("key_reserved", "integer default 0"),
            ("account_id", "text default ''"),
        ):
            if name not in columns:
                self._run(f"alter table api_jobs add column {name} {declaration}")
        columns = {row["name"] for row in self.all("pragma table_info(payments)")}
        if "mode" not in columns:
            self._run("alter table payments add column mode text default 'real'")

    def _run(self, sql: str, args: tuple = ()) -> sqlite3.Cursor:
        cur = self.conn.execute(sql, args)
        self.conn.commit()
        return cur

    def one(self, sql: str, args: tuple = ()) -> sqlite3.Row | None:
        with closing(self.conn.execute(sql, args)) as cur:
            return cur.fetchone()

    def all(self, sql: str, args: tuple = ()) -> list[sqlite3.Row]:
        with closing(self.conn.execute(sql, args)) as cur:
            return cur.fetchall()

    # --- пользователи -----------------------------------------------------
    def user(self, tg_id: int, username: str = "", name: str = "") -> sqlite3.Row:
        self._run(
            "insert into users (tg_id, username, name, created_at) values (?,?,?,?) "
            "on conflict(tg_id) do update set username=excluded.username, name=excluded.name",
            (tg_id, username, name, now()),
        )
        row = self.one("select * from users where tg_id=?", (tg_id,))
        assert row is not None
        return row

    def link_identity(self, email: str, tg_id: int) -> None:
        """Запоминает, что этот Apple ID открывали из этого телеграм-аккаунта."""
        email = email.strip().lower()
        if not email:
            return
        self._run(
            "insert into identities (email, tg_id, first_seen, last_seen) values (?,?,?,?) "
            "on conflict(email, tg_id) do update set last_seen=excluded.last_seen",
            (email, tg_id, now(), now()),
        )

    def family(self, tg_id: int) -> list[int]:
        """Все телеграм-аккаунты, которые ходили под теми же Apple ID.

        Бесплатные установки считаются на человека, а не на аккаунт: иначе их
        достаточно перевыпустить в новом телеграме, оставив тот же Apple ID.
        """
        rows = self.all(
            "with recursive family(tg_id) as (select ? union select b.tg_id "
            "from family f join identities a on a.tg_id=f.tg_id join identities b on b.email=a.email) "
            "select tg_id from family",
            (tg_id,),
        )
        ids = sorted({int(row["tg_id"]) for row in rows} | {tg_id})
        return ids

    def week_uses(self, tg_id: int) -> int:
        ids = self.family(tg_id)
        marks = ",".join("?" * len(ids))
        row = self.one(
            f"select count(*) c from history where tg_id in ({marks}) "
            "and status='ok' and paid='free' and created_at>?",
            (*ids, now() - WEEK),
        )
        claimed = self.one(
            f"select count(*) c from download_claims where tg_id in ({marks}) and paid='free' and expires_at>?",
            (*ids, now()),
        )
        return (int(row["c"]) if row else 0) + int(claimed["c"])

    def sub_active(self, tg_id: int) -> int:
        row = self.one("select sub_until from users where tg_id=?", (tg_id,))
        until = int(row["sub_until"]) if row else 0
        return until if until > now() else 0

    def credits(self, tg_id: int) -> int:
        row = self.one("select credits from users where tg_id=?", (tg_id,))
        return int(row["credits"]) if row else 0

    def add_credits(self, tg_id: int, count: int) -> None:
        self._run(
            "update users set credits=max(0, credits+?) where tg_id=?", (count, tg_id)
        )

    def extend_sub(self, tg_id: int, days: int) -> int:
        base = max(self.sub_active(tg_id), now())
        until = base + days * 24 * 3600
        self._run("update users set sub_until=? where tg_id=?", (until, tg_id))
        return until

    def blocked(self, tg_id: int) -> bool:
        row = self.one("select blocked from users where tg_id=?", (tg_id,))
        return bool(row["blocked"]) if row else False

    def set_blocked(self, tg_id: int, flag: bool) -> None:
        self._run("update users set blocked=? where tg_id=?", (1 if flag else 0, tg_id))

    def users_page(self, limit: int = 8, offset: int = 0) -> list[sqlite3.Row]:
        return self.all(
            "select u.*, (select count(*) from history h where h.tg_id=u.tg_id and h.status='ok') uses"
            " from users u order by u.created_at desc limit ? offset ?",
            (limit, offset),
        )

    def users_count(self) -> int:
        row = self.one("select count(*) c from users")
        return int(row["c"]) if row else 0

    def user_ids(self) -> list[int]:
        return [
            int(row["tg_id"])
            for row in self.all(
                "select tg_id from users where blocked=0 and api_only=0"
            )
        ]

    def files_stats(self) -> tuple[int, int]:
        row = self.one("select count(*) c, coalesce(sum(size),0) s from files")
        return (int(row["c"]), int(row["s"])) if row else (0, 0)

    def files_all(self) -> list[sqlite3.Row]:
        return self.all("select * from files")

    # --- аккаунты Apple ---------------------------------------------------
    def account_add(self, tg_id: int, email: str, password: str, keychain: str) -> int:
        email = email.strip().casefold()
        self._run(
            "insert into accounts (tg_id, email, password, keychain, created_at) values (?,?,?,?,?) "
            "on conflict(tg_id,email) do update set password=excluded.password",
            (tg_id, email, password, keychain, now()),
        )
        row = self.one(
            "select id from accounts where email=? and tg_id=?", (email, tg_id)
        )
        return int(row["id"]) if row else 0

    def accounts(self) -> list[sqlite3.Row]:
        return self.all("select * from accounts order by id")

    def kbsync_due(self, interval: int = 12 * 3600) -> list[sqlite3.Row]:
        return self.all(
            "select a.* from accounts a join users u on u.tg_id=a.tg_id "
            "where a.status='ready' and u.blocked=0 and u.sub_until>? "
            "and a.kbsync_warmed_at<=? order by a.kbsync_warmed_at, a.id",
            (now(), now() - interval),
        )

    def kbsync_warmed(self, account_id: int) -> None:
        self._run(
            "update accounts set kbsync_warmed_at=? where id=?", (now(), account_id)
        )

    def kbsync_reset(self, account_id: int) -> None:
        self._run("update accounts set kbsync_warmed_at=0 where id=?", (account_id,))

    def account(self, account_id: int) -> sqlite3.Row | None:
        return self.one("select * from accounts where id=?", (account_id,))

    def account_by_email(self, email: str) -> sqlite3.Row | None:
        return self.one("select * from accounts where email=?", (email,))

    def account_status(self, account_id: int, status: str, note: str = "") -> None:
        self._run(
            "update accounts set status=?, note=? where id=?",
            (status, note, account_id),
        )

    def account_of(self, tg_id: int) -> sqlite3.Row | None:
        """Apple ID этого пользователя: рабочий в приоритете."""
        return self.one(
            "select * from accounts where tg_id=? order by (status='ready') desc, id limit 1",
            (tg_id,),
        )

    def accounts_of(self, tg_id: int) -> list[sqlite3.Row]:
        return self.all("select * from accounts where tg_id=? order by id", (tg_id,))

    def account_used(self, account_id: int) -> None:
        self._run(
            "update accounts set uses=uses+1, last_used=? where id=?",
            (now(), account_id),
        )

    def account_delete(self, account_id: int) -> None:
        self._run("delete from apple_web_sessions where account_id=?", (account_id,))
        self._run("delete from ownership_cache where account_id=?", (account_id,))
        self._run("delete from accounts where id=?", (account_id,))

    # --- приложения -------------------------------------------------------
    def app_save(self, app: dict[str, Any]) -> None:
        self._run(
            "insert into apps (adam_id, bundle_id, title, artist, icon, version, min_ios, size, source, updated_at)"
            " values (?,?,?,?,?,?,?,?,?,?)"
            " on conflict(adam_id) do update set bundle_id=excluded.bundle_id, title=excluded.title,"
            " artist=excluded.artist, icon=excluded.icon, version=excluded.version, min_ios=excluded.min_ios,"
            " size=excluded.size, source=excluded.source, updated_at=excluded.updated_at",
            (
                str(app.get("adam_id", "")),
                app.get("bundle_id", "") or "",
                app.get("title", "") or "",
                app.get("artist", "") or "",
                app.get("icon", "") or "",
                app.get("version", "") or "",
                app.get("min_ios", "") or "",
                int(app.get("size") or 0),
                app.get("source", "") or "",
                now(),
            ),
        )

    def app(self, adam_id: str) -> sqlite3.Row | None:
        return self.one("select * from apps where adam_id=?", (str(adam_id),))

    # --- файлы ------------------------------------------------------------
    def file_add(self, token: str, **fields: Any) -> None:
        keys = [
            "adam_id",
            "account_id",
            "tg_id",
            "name",
            "path",
            "size",
            "bundle_id",
            "version",
            "title",
            "signed",
            "encrypted",
            "expires_at",
            "paid_with",
            "ttl_hours",
        ]
        numeric = {
            "account_id",
            "tg_id",
            "size",
            "signed",
            "encrypted",
            "expires_at",
            "ttl_hours",
        }
        values = [fields.get(key, 0 if key in numeric else "") for key in keys]
        placeholders = ", ".join(["?"] * (len(keys) + 2))
        self._run(
            f"insert or replace into files (token, {', '.join(keys)}, created_at) values ({placeholders})",
            (token, *values, now()),
        )

    def file(self, token: str) -> sqlite3.Row | None:
        return self.one("select * from files where token=?", (token,))

    def files_expired(self) -> list[sqlite3.Row]:
        return self.all(
            "select * from files where expires_at>0 and expires_at<=?", (now(),)
        )

    def file_delete(self, token: str) -> None:
        self._run("delete from files where token=?", (token,))

    # --- история ----------------------------------------------------------
    def history_add(
        self,
        tg_id: int,
        adam_id: str,
        title: str,
        kind: str,
        account_id: int,
        status: str,
        paid: str = "",
    ) -> int:
        cur = self._run(
            "insert into history (tg_id, adam_id, title, kind, account_id, status, paid, created_at)"
            " values (?,?,?,?,?,?,?,?)",
            (tg_id, str(adam_id), title, kind, account_id, status, paid, now()),
        )
        return int(cur.lastrowid or 0)

    def history(self, tg_id: int, limit: int = 20) -> list[sqlite3.Row]:
        return self.all(
            "select * from history where tg_id=? order by id desc limit ?",
            (tg_id, limit),
        )

    def payment_add(
        self, tg_id: int, plan: str, amount: int, currency: str, charge_id: str
    ) -> None:
        self._run(
            "insert into payments (tg_id, plan, amount, currency, charge_id, created_at) values (?,?,?,?,?,?)",
            (tg_id, plan, amount, currency, charge_id, now()),
        )

    def stats(self) -> dict[str, int]:
        def scalar(sql: str, args: tuple = ()) -> int:
            row = self.one(sql, args)
            return int(row[0]) if row else 0

        return {
            "users": scalar("select count(*) from users"),
            "accounts": scalar("select count(*) from accounts"),
            "accounts_ready": scalar(
                "select count(*) from accounts where status='ready'"
            ),
            "users_week": scalar(
                "select count(*) from users where created_at>?", (now() - WEEK,)
            ),
            "downloads": scalar("select count(*) from history where status='ok'"),
            "week": scalar(
                "select count(*) from history where status='ok' and created_at>?",
                (now() - WEEK,),
            ),
            "revenue": scalar(
                "select coalesce(sum(amount),0) from payments where mode='real' and currency='RUB'"
            ),
            "demo_payments": scalar("select count(*) from payments where mode='demo'"),
        }
