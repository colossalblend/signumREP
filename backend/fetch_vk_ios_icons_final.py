                      
"""Fetch real iOS icons for VK apps via AppMagic applications/app-info and patch common.php."""
from __future__ import annotations

import os
import json
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

sys.stdout.reconfigure(line_buffering=True)

ROOT = Path(__file__).resolve().parent
COMMON = ROOT / "signum" / "common.php"

TOKEN = os.environ.get("APPMAGIC_TOKEN", "")
UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/148.0.0.0 YaBrowser/26.6.0.0 Safari/537.36"
)
COOKIE = os.environ.get("APPMAGIC_COOKIE", "")

APPS = [
    "564177498",
    "398465290",
    "1016489154",
    "511310430",
    "1054372220",
    "6447614666",
    "696551382",
    "1441659687",
    "1343242452",
    "6449036810",
    "1467719381",
    "1563419339",
    "1615733452",
    "1459096573",
    "1513171161",
]


def api_post(url: str, data: dict) -> dict | list | str:
    headers = {
        "accept": "application/json, text/plain, */*",
        "authorization": f"Bearer {TOKEN}",
        "content-type": "application/json",
        "content-language": "en",
        "accept-language": "ru,en;q=0.9",
        "origin": "https://appmagic.rocks",
        "referer": "https://appmagic.rocks/",
        "user-agent": UA,
        "cookie": COOKIE,
    }
    body = json.dumps(data).encode()
    req = urllib.request.Request(url, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=40) as resp:
            raw = resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", "replace")
        print(f"  HTTP {e.code}: {raw[:300]}", flush=True)
        try:
            return json.loads(raw)
        except Exception:
            return raw
    if raw.lstrip().startswith("<!"):
        return raw
    return json.loads(raw)


def to_300(icon: str) -> str:
    icon = icon.strip()
                                 
    for p in (
        "https://images.appmagic.rocks/?uri=",
        "http://images.appmagic.rocks/?uri=",
    ):
        if icon.startswith(p):
            icon = urllib.parse.unquote(icon[len(p) :])
    if "mzstatic.com" not in icon:
        return ""
    icon = re.sub(r"/0x\d+h\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    icon = re.sub(r"/\d+x\d+bb\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    icon = re.sub(r"/\d+x\d+\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    return icon


def extract_icon(obj) -> str:
    """Walk JSON for best mzstatic icon_url."""
    found: list[str] = []

    def walk(x):
        if isinstance(x, dict):
            for k, v in x.items():
                if k in ("icon_url", "icon", "artwork", "image", "iconUrl") and isinstance(v, str):
                    if "mzstatic.com" in v:
                        found.append(v)
                else:
                    walk(v)
        elif isinstance(x, list):
            for i in x:
                walk(i)

    walk(obj)
    if not found:
        return ""
                    
    preferred = [u for u in found if "AppIcon" in u]
    return to_300((preferred or found)[0])


def fetch_icon(app_id: str) -> str:
    url = "https://appmagic.rocks/api/v2/applications/app-info"
                                                       
    payloads = []
    for store in (2, 3, "2", "iphone", "ios", "apple"):
        for country in ("RU", "US", "WW", "KZ"):
            payloads.append(
                {
                    "storeApplicationId": str(app_id),
                    "store": store,
                    "country": country,
                }
            )
            payloads.append(
                {
                    "StoreApplicationID": str(app_id),
                    "Store": store,
                    "Country": country,
                }
            )
            payloads.append(
                {
                    "store_application_id": str(app_id),
                    "store": store,
                    "country": country,
                }
            )

    seen = set()
    for payload in payloads:
        key = json.dumps(payload, sort_keys=True)
        if key in seen:
            continue
        seen.add(key)
        data = api_post(url, payload)
        if isinstance(data, str):
            continue
        if isinstance(data, dict) and data.get("message"):
                         
            msg = str(data.get("message"))
            if "forbidden" in msg.lower():
                print(f"  forbidden with {payload}", flush=True)
                break
            continue
        icon = extract_icon(data)
        if icon:
            print(f"  OK payload={payload} icon={icon}", flush=True)
            return icon
                                  
        if isinstance(data, dict):
            print(f"  got data keys={list(data.keys())[:20]} payload={payload}", flush=True)
                          
            sample = json.dumps(data, ensure_ascii=False)[:400]
            print(f"  sample={sample}", flush=True)
                                                         
            if "icon" in sample.lower() or "name" in sample.lower():
                break
    return ""


def patch_common(mapping: dict[str, str]) -> int:
    text = COMMON.read_text(encoding="utf-8")
    changed = 0
    for app_id, icon in mapping.items():
        if not icon:
            continue
        idx = 0
        while True:
            pos = text.find(f"'app_store_id' => '{app_id}'", idx)
            if pos < 0:
                break
            start = max(0, pos - 1500)
            end = min(len(text), pos + 800)
            chunk = text[start:end]

            def repl(m: re.Match) -> str:
                nonlocal changed
                old = m.group(2)
                if old != icon:
                    changed += 1
                return m.group(1) + icon + m.group(3)

            new_chunk = re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", repl, chunk)
            text = text[:start] + new_chunk + text[end:]
            idx = pos + 40

                                              
    vk_start = text.find("'vk' => [")
    if vk_start > 0:
        vk_end = text.find("\n    ];\n\n    // Добавляем active", vk_start)
        if vk_end > vk_start:
            section = text[vk_start:vk_end]

            def clean(m: re.Match) -> str:
                nonlocal changed
                old = m.group(2)
                new = old
                if "images.appmagic.rocks/?uri=" in new:
                    new = urllib.parse.unquote(
                        new.split("images.appmagic.rocks/?uri=", 1)[1]
                    )
                if "mzstatic.com" in new:
                    new2 = to_300(new)
                    if new2:
                        new = new2
                if new != old:
                    changed += 1
                return m.group(1) + new + m.group(3)

            section = re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", clean, section)
            text = text[:vk_start] + section + text[vk_end:]

    COMMON.write_text(text, encoding="utf-8")
    return changed


def main() -> None:
                                        
    print("=== debug single request ===", flush=True)
    for payload in (
        {"storeApplicationId": "564177498", "store": 2, "country": "RU"},
        {"storeApplicationId": "564177498", "store": 2, "country": "US"},
        {"StoreApplicationID": "564177498", "Store": 2, "Country": "RU"},
        {"store_application_id": "564177498", "store": 2, "country": "RU"},
        {"storeApplicationId": "564177498", "store": 1, "country": "RU"},
    ):
        print("payload", payload, flush=True)
        data = api_post("https://appmagic.rocks/api/v2/applications/app-info", payload)
        print(type(data), str(data)[:500], flush=True)

    mapping: dict[str, str] = {}
    for app_id in APPS:
        print(f"\n== {app_id}", flush=True)
        icon = fetch_icon(app_id)
        if not icon:
            print("  FAILED", flush=True)
        mapping[app_id] = icon
        time.sleep(0.15)

    (ROOT / "vk_ios_icons.json").write_text(
        json.dumps(mapping, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    n = patch_common(mapping)
    ok = sum(1 for v in mapping.values() if v)
    print(f"\nDone. icons={ok}/{len(mapping)} patched~{n}", flush=True)
    for k, v in mapping.items():
        print(f"  {k}: {v or 'MISSING'}", flush=True)


if __name__ == "__main__":
    main()
