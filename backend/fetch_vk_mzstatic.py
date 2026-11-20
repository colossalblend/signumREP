                      
"""Get real iOS mzstatic icons for VK apps via Wayback / other sources, patch common.php."""
from __future__ import annotations

import os
import json
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent
COMMON = ROOT / "signum" / "common.php"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36"

APPS = {
    "564177498": "vk",
    "398465290": "ok",
    "1016489154": "yula",
    "511310430": "mail",
    "1054372220": "vkmusic",
    "6447614666": "vkvideo",
    "696551382": "cloud",
    "1441659687": "vkmessenger",
    "1343242452": "dzen",
    "6449036810": "vkdating",
    "1467719381": "marusya",
    "1563419339": "vkmail",
    "1615733452": "vkclips",
    "1459096573": "vkws",
    "1513171161": "vkplay",
}


def http(url: str, timeout: int = 45) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return resp.read().decode("utf-8", "replace")


def to_300(icon: str) -> str:
    icon = icon.strip().rstrip("\\")
    icon = re.sub(r"/0x\d+h\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    icon = re.sub(r"/\d+x\d+bb\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    icon = re.sub(r"/\d+x\d+\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    return icon


def extract_icons(html: str) -> list[str]:
    found = re.findall(
        r"https://is\d+-ssl\.mzstatic\.com/image/thumb/[^\"'\\s<>]+",
        html,
    )
    out: list[str] = []
    for u in found:
        u = to_300(u)
                                       
        if u not in out:
            out.append(u)
                                     
    preferred = [u for u in out if "AppIcon" in u or ("Purple" in u and "Placeholder" not in u)]
    return preferred or out


def wayback_icon(app_id: str) -> str:
    queries = [
        f"https://web.archive.org/cdx/search/cdx?url=apps.apple.com/*/id{app_id}*&output=json&filter=statuscode:200&limit=20&fl=timestamp,original&from=2022",
        f"https://web.archive.org/cdx/search/cdx?url=apps.apple.com/app/id{app_id}&output=json&filter=statuscode:200&limit=20&fl=timestamp,original",
        f"https://web.archive.org/cdx/search/cdx?url=apps.apple.com/ru/app/id{app_id}&output=json&limit=10&fl=timestamp,original",
        f"https://web.archive.org/cdx/search/cdx?url=itunes.apple.com/*/id{app_id}*&output=json&filter=statuscode:200&limit=10&fl=timestamp,original",
    ]
    snapshots: list[tuple[str, str]] = []
    for q in queries:
        try:
            data = json.loads(http(q))
            if not data or len(data) <= 1:
                continue
            for row in data[1:]:
                snapshots.append((row[0], row[1]))
        except Exception as e:
            print(f"  cdx err: {e}")
    if not snapshots:
        return ""
                  
    snapshots.sort(key=lambda x: x[0], reverse=True)
    for ts, original in snapshots[:6]:
        snap = f"https://web.archive.org/web/{ts}id_/{original}"
        try:
            html = http(snap)
            icons = extract_icons(html)
            if icons:
                                   
                print(f"  wayback {ts}: {icons[0][:100]}")
                return icons[0]
            m = re.search(
                r'property=["\']og:image["\'][^>]*content=["\']([^"\']+)["\']',
                html,
                re.I,
            )
            if not m:
                m = re.search(
                    r'content=["\']([^"\']+)["\'][^>]*property=["\']og:image["\']',
                    html,
                    re.I,
                )
            if m and "mzstatic.com" in m.group(1):
                return to_300(m.group(1))
        except Exception as e:
            print(f"  snap err {ts}: {e}")
            continue
    return ""


def appmagic_search_icon(app_id: str) -> str:
    """Only accept mzstatic from search match itself (not similar apps)."""
    TOKEN = os.environ.get("APPMAGIC_TOKEN", "")
    COOKIE = os.environ.get("APPMAGIC_COOKIE", "")
    url = f"https://appmagic.rocks/api/v2/search?name={app_id}&limit=20"
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": UA,
            "authorization": f"Bearer {TOKEN}",
            "cookie": COOKIE,
            "accept": "application/json",
            "origin": "https://appmagic.rocks",
            "referer": "https://appmagic.rocks/",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read().decode())
        for app in data.get("applications", []):
            sids = [str(x) for x in app.get("store_application_ids", [])]
            if app_id not in sids:
                continue
            icon = app.get("icon_url") or ""
            if "mzstatic.com" in icon:
                return to_300(icon)
            return ""                        
    except Exception as e:
        print(f"  am search err: {e}")
    return ""


def itunes_lookup(app_id: str) -> str:
    for country in ("ru", "us", "kz", "by", "am", "uz", "tj", "kg"):
        try:
            url = f"https://itunes.apple.com/lookup?id={app_id}&country={country}"
            data = json.loads(http(url, timeout=20))
            if data.get("resultCount", 0) > 0:
                art = data["results"][0].get("artworkUrl100") or ""
                if "mzstatic.com" in art:
                    return to_300(art)
        except Exception:
            pass
    return ""


def main() -> None:
    mapping: dict[str, str] = {}
    for app_id, name in APPS.items():
        print(f"== {app_id} {name}")
        icon = itunes_lookup(app_id)
        if icon:
            print(f"  itunes: {icon}")
        if not icon:
            icon = appmagic_search_icon(app_id)
            if icon:
                print(f"  appmagic mzstatic: {icon}")
        if not icon:
            icon = wayback_icon(app_id)
            if icon:
                print(f"  wayback: {icon}")
        if not icon:
            print("  FAILED")
        mapping[app_id] = icon
        time.sleep(0.3)

    out = ROOT / "vk_ios_icons.json"
    out.write_text(json.dumps(mapping, ensure_ascii=False, indent=2), encoding="utf-8")
    print("WROTE", out)

                      
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
            end = min(len(text), pos + 700)
            chunk = text[start:end]

            def repl(m: re.Match) -> str:
                nonlocal changed
                old = m.group(2)
                if old != icon:
                    changed += 1
                return m.group(1) + icon + m.group(3)

            new_chunk = re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", repl, chunk)
            text = text[:start] + new_chunk + text[end:]
            idx = pos + len(app_id) + 20

                                                                                             
    def cleanup(m: re.Match) -> str:
        nonlocal changed
        old = m.group(2)
        new = old
        if new.startswith("https://images.appmagic.rocks/?uri="):
            new = urllib.parse.unquote(new[len("https://images.appmagic.rocks/?uri=") :])
        if "mzstatic.com" in new:
            new = to_300(new)
        if new != old:
            changed += 1
        return m.group(1) + new + m.group(3)

                                               
    vk_start = text.find("'vk' => [")
    if vk_start > 0:
                                           
        vk_end = text.find("\n    ];\n\n    // Добавляем active", vk_start)
        if vk_end > vk_start:
            section = text[vk_start:vk_end]
            section2 = re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", cleanup, section)
            text = text[:vk_start] + section2 + text[vk_end:]

    COMMON.write_text(text, encoding="utf-8")
    ok = sum(1 for v in mapping.values() if v)
    print(f"Patched common.php (~{changed} icon writes). Found {ok}/{len(mapping)}")
    for k, v in mapping.items():
        print(f"  {k}: {v or 'MISSING'}")


if __name__ == "__main__":
    main()
