import unittest

from aiohttp import web

from signum_bot.bridge_dispatch import dispatch


class DispatchTests(unittest.IsolatedAsyncioTestCase):
    async def test_preserves_authentication_json_and_client_address(self):
        @web.middleware
        async def guard(request, handler):
            if request.headers.get("Authorization") != "Bearer fixture":
                return web.json_response({"error": "invalid_api_key"}, status=401)
            return await handler(request)
        async def echo(request):
            data = await request.json()
            return web.json_response({"value": data["value"], "peer": request.remote,
                                      "client": request.headers["X-Real-IP"]})
        app = web.Application(middlewares=[guard])
        app.router.add_post("/api/v1/echo", echo)
        runner = web.AppRunner(app)
        await runner.setup()
        try:
            denied = await dispatch(app, "POST", "/api/v1/echo", b"{}", {})
            self.assertEqual(denied.status_code, 401)
            response = await dispatch(app, "POST", "/api/v1/echo", b'{"value":42}',
                                      {"Authorization": "Bearer fixture", "Content-Type": "application/json", "X-Real-IP": "192.0.2.10"})
            self.assertEqual(response.status_code, 200)
            self.assertEqual(response.json(), {"value": 42, "peer": "127.0.0.1", "client": "192.0.2.10"})
            missing = await dispatch(app, "GET", "/api/v1/missing", b"", {"Authorization": "Bearer fixture"})
            self.assertEqual(missing.status_code, 404)
            with self.assertRaises(ValueError):
                await dispatch(app, "POST", "/api/v1/echo", b"x" * 16385, {})
        finally:
            await runner.cleanup()
