from __future__ import annotations

import time


def take_request(cache, key, limit: int, *, period: float = 60) -> bool:
    moment = time.monotonic()
    count, reset = cache.get(key, (0, moment + period))
    if moment >= reset:
        count, reset = 0, moment + period
    if count >= limit:
        return False
    cache[key] = (count + 1, reset)
    return True
