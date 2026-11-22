from __future__ import annotations

import asyncio
from types import SimpleNamespace

import httpx
from aiohttp import web, StreamReader
from aiohttp.http_parser import RawRequestMessage
from aiohttp.http_writer import HttpVersion
from multidict import CIMultiDict, CIMultiDictProxy
from yarl import URL


async def dispatch(app, method, path, body, headers):
    if len(body) > 16384:
        raise ValueError("Bridge request too large")
    loop = asyncio.get_running_loop()
    protocol = SimpleNamespace(peername=("127.0.0.1", 0), ssl_context=None,
                               transport=None, _reading_paused=False)
    payload = StreamReader(protocol, 32768, loop=loop)
    payload.feed_data(body)
    payload.feed_eof()
    values = CIMultiDict(headers)
    values["Content-Length"] = str(len(body))
    values.setdefault("Host", "localhost")
    raw_headers = tuple((k.encode(), v.encode()) for k, v in values.items())
    message = RawRequestMessage(method, path, HttpVersion(1, 1),
                                CIMultiDictProxy(values), raw_headers,
                                False, None, False, False, URL(path))
    request = web.Request(message, payload, protocol, None,
                          asyncio.current_task(), loop, client_max_size=16384)
    try:
        response = await app._handle(request)
    except web.HTTPException as error:
        response = error
    if not isinstance(response, web.Response):
        raise ValueError("Streaming API responses are not supported by the bridge")
    return httpx.Response(response.status, content=response.body or b"",
                          headers=dict(response.headers),
                          request=httpx.Request(method, "http://localhost" + path))
