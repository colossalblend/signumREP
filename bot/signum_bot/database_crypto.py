from __future__ import annotations

import os
import sqlite3
from pathlib import Path


def database_key():
    name = os.environ.get("SIGNUM_DB_KEY_FILE", "")
    if not name:
        return None
    key = Path(name).read_bytes()
    if len(key) != 32:
        raise RuntimeError("Invalid database encryption key")
    return key


def database_driver():
    if database_key() is None:
        return sqlite3
    from sqlcipher3 import dbapi2

    return dbapi2


def connect_database(path, **kwargs):
    key = database_key()
    driver = database_driver()
    connection = driver.connect(path, **kwargs)
    if key is not None:
        try:
            connection.execute("PRAGMA cipher_log_level=ERROR")
            connection.execute(f'PRAGMA key = "x\'{key.hex()}\'"')
            connection.execute("PRAGMA cipher_memory_security=ON")
            connection.execute("PRAGMA secure_delete=ON")
            connection.execute("select count(*) from sqlite_master").fetchone()
        except Exception:
            connection.close()
            raise
    connection.row_factory = driver.Row
    return connection
