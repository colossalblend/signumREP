from __future__ import annotations

import os


class ApiLease:
    def __init__(self, path, message=None):
        path.parent.mkdir(parents=True, exist_ok=True)
        self.stream = open(path, "a+b")
        if self.stream.tell() == 0:
            self.stream.write(b"0")
            self.stream.flush()
        self.stream.seek(0)
        try:
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(self.stream.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(self.stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            self.stream.close()
            raise RuntimeError(
                message
                or "API уже запущен для этой базы. Для отдельного сервиса отключите API_EMBEDDED у бота."
            ) from None

    def close(self):
        if self.stream.closed:
            return
        self.stream.seek(0)
        if os.name == "nt":
            import msvcrt

            msvcrt.locking(self.stream.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(self.stream, fcntl.LOCK_UN)
        self.stream.close()
