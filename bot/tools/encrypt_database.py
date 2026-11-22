from __future__ import annotations

import os
import sqlite3
import sys
from contextlib import closing
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from signum_bot.config import Config
from signum_bot.database_crypto import database_key, connect_database
from signum_bot.production import backup_database


def main():
    cfg = Config.load()
    key = database_key()
    if key is None:
        raise SystemExit("SIGNUM_DB_KEY_FILE is required")
    path = cfg.db_path
    if path.read_bytes()[:16] != b"SQLite format 3\x00":
        with closing(connect_database(path)) as encrypted:
            if encrypted.execute("pragma quick_check").fetchone()[0] != "ok":
                raise RuntimeError("Encrypted database integrity failure")
        print("Database already encrypted and verified")
        return
    # Backup the plaintext source into an encrypted destination before replacing it.
    from sqlcipher3 import dbapi2 as sqlcipher
    target = path.with_suffix(".encrypted.tmp")
    if target.exists():
        raise RuntimeError("Migration output already exists")
    with closing(sqlite3.connect(path)) as source:
        source.execute("pragma wal_checkpoint(truncate)")
        source_counts = {
            name: source.execute('select count(*) from "' + name.replace('"', '""') + '"').fetchone()[0]
            for (name,) in source.execute("select name from sqlite_master where type='table'")
        }
    with closing(sqlcipher.connect(path)) as source:
        escaped = str(target).replace("'", "''")
        source.execute(f"ATTACH DATABASE '{escaped}' AS protected KEY \"x'{key.hex()}'\"")
        source.execute("select sqlcipher_export('protected')")
        source.commit()
        source.execute("DETACH DATABASE protected")
    os.chmod(target, 0o600)
    with closing(connect_database(target)) as encrypted:
        if encrypted.execute("pragma quick_check").fetchone()[0] != "ok":
            raise RuntimeError("Encrypted database integrity failure")
        for name, count in source_counts.items():
            actual = encrypted.execute('select count(*) from "' + name.replace('"', '""') + '"').fetchone()[0]
            if actual != count:
                raise RuntimeError("Database row count mismatch")
    # The SQLCipher snapshot itself is the recoverable copy, with another encrypted backup.
    from signum_bot.api_secrets import ApiSecrets
    import gzip
    vault = ApiSecrets(cfg.data_dir)
    backup = cfg.data_dir / "backups" / "pre-encryption.db.gz.enc"
    backup.parent.mkdir(parents=True, exist_ok=True)
    with backup.open("xb") as stream:
        os.chmod(backup, 0o600)
        stream.write(vault.cipher.encrypt(gzip.compress(path.read_bytes())))
    os.replace(target, path)
    print("Encrypted database verified; all table counts preserved")


if __name__ == "__main__":
    main()
