                      
"""Probe AppMagic for real iOS icons of a store app id."""
from __future__ import annotations

import os
import json
import re
import urllib.parse
import urllib.request

TOKEN = os.environ.get("APPMAGIC_TOKEN", "")
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36"
COOKIE = os.environ.get("APPMAGIC_COOKIE", "")

APP_ID = "564177498"


def req(url: str, data=None, method=None) -> tuple[int, str]:
    headers = {
        "accept": "application/json, text/plain, */*",
        "authorization": f"Bearer {TOKEN}",
        "content-language": "en",
        "accept-language": "ru,en;q=0.9",
        "origin": "https://appmagic.rocks",
        "referer": f"https://appmagic.rocks/iphone/vk-social-network-messenger/{APP_ID}?infoCountry=RU",
        "user-agent": UA,
        "cookie": COOKIE,
    }
    body = None
    if data is not None:
        headers["content-type"] = "application/json"
        body = json.dumps(data).encode()
        method = method or "POST"
    r = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(r, timeout=30) as resp:
            return resp.status, resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")[:2000]


candidates = [
    f"https://appmagic.rocks/api/v2/search?name={APP_ID}&limit=5",
    f"https://appmagic.rocks/api/v2/search?store_application_id={APP_ID}&limit=5",
    f"https://appmagic.rocks/api/v2/search?q={APP_ID}&limit=5",
    f"https://appmagic.rocks/api/v2/search?name=VK&store=2&limit=20",
    f"https://appmagic.rocks/api/v2/applications/{APP_ID}",
    f"https://appmagic.rocks/api/v2/application/{APP_ID}",
    f"https://appmagic.rocks/api/v2/united_application/{APP_ID}",
    f"https://appmagic.rocks/api/v2/united-applications/{APP_ID}",
    f"https://appmagic.rocks/api/v2/store_application/{APP_ID}",
    f"https://appmagic.rocks/api/v2/store-applications/{APP_ID}",
    f"https://appmagic.rocks/api/v2/store-application?id={APP_ID}&store=2",
    f"https://appmagic.rocks/api/v2/app-info?id={APP_ID}",
    f"https://appmagic.rocks/api/v2/app-info?store_application_id={APP_ID}&store=2",
    f"https://appmagic.rocks/api/v2/info/app/{APP_ID}",
    f"https://appmagic.rocks/api/v2/info?id={APP_ID}&store=2",
    f"https://appmagic.rocks/api/v2/top-info?id={APP_ID}",
    f"https://appmagic.rocks/api/v2/base-info?id={APP_ID}",
    f"https://appmagic.rocks/api/v2/application-base?id={APP_ID}",
    f"https://appmagic.rocks/api/v2/application-base-info?store_application_id={APP_ID}",
    f"https://appmagic.rocks/api/v2/live-ops/app-info?id={APP_ID}",
]

                    
posts = [
    ("https://appmagic.rocks/api/v2/application-info", {"id": APP_ID}),
    ("https://appmagic.rocks/api/v2/application-info", {"store_application_id": APP_ID, "store": 2}),
    ("https://appmagic.rocks/api/v2/app-info", {"id": APP_ID}),
    ("https://appmagic.rocks/api/v2/app-info", {"id": APP_ID, "store": 2}),
    ("https://appmagic.rocks/api/v2/united-application-info", {"id": APP_ID}),
    ("https://appmagic.rocks/api/v2/info", {"id": APP_ID, "store": 2}),
    ("https://appmagic.rocks/api/v2/base", {"id": APP_ID}),
]

for url in candidates:
    code, body = req(url)
    mz = "mzstatic" in body
    play = "play-lh" in body
    print(f"GET {code} mz={mz} play={play} {url}")
    if code == 200 and (mz or "icon" in body[:500].lower()):
                           
        try:
            data = json.loads(body)
            print("  keys:", list(data.keys())[:30] if isinstance(data, dict) else type(data))
            text = json.dumps(data, ensure_ascii=False)
            icons = re.findall(r'https://[^"\\]+(?:mzstatic|play-lh)[^"\\]+', text)
            for i in icons[:8]:
                print("  ICON:", i[:140])
        except Exception as e:
            print("  parse err", e, body[:200])

for url, data in posts:
    code, body = req(url, data=data)
    mz = "mzstatic" in body
    play = "play-lh" in body
    print(f"POST {code} mz={mz} play={play} {url} {data}")
    if code == 200:
        try:
            j = json.loads(body)
            print("  keys:", list(j.keys())[:30] if isinstance(j, dict) else type(j))
            text = json.dumps(j, ensure_ascii=False)
            icons = re.findall(r'https://[^"\\]+(?:mzstatic|play-lh)[^"\\]+', text)
            for i in icons[:8]:
                print("  ICON:", i[:140])
        except Exception as e:
            print("  ", body[:300])

                                                           
for term in ["ВКонтакте", "VK messenger", "VK: music video messenger", "com.vk.vkclient"]:
    url = "https://itunes.apple.com/search?term=" + urllib.parse.quote(term) + "&entity=software&country=ru&limit=5"
    code, body = req(url)
    print(f"ITUNES SEARCH {code} {term}")
    try:
        data = json.loads(body)
        for r in data.get("results", [])[:5]:
            print(" ", r.get("trackId"), r.get("trackName"), r.get("bundleId"), r.get("artworkUrl100"))
    except Exception as e:
        print(" ", e, body[:200])
