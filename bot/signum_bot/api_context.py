from aiohttp import web

OWNER = web.AppKey("api_owner", int)
SCOPES = web.AppKey("api_scopes", frozenset)
KEY_ID = web.AppKey("api_key_id", str)


class ApiError(Exception):
    def __init__(self, status: int, code: str):
        self.status = status
        self.code = code
