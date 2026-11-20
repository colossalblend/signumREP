                      
"""Fetch iOS (mzstatic) icons for VK catalog apps and patch common.php."""
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

TOKEN = os.environ.get("APPMAGIC_TOKEN", "")
UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/148.0.0.0 YaBrowser/26.6.0.0 Safari/537.36"
)
COOKIE = os.environ.get("APPMAGIC_COOKIE", "")

                                
APPS = [
    ("564177498", "vk-social-network-messenger"),
    ("398465290", "odnoklassniki-social-network"),
    ("1016489154", "yula-obuyavleniya-ryadom"),
    ("511310430", "mail-email-cloud-calendar"),
    ("1054372220", "vk-music-playlists-and-podcasts"),
    ("6447614666", "vk-video-shows-films-series"),
    ("696551382", "cloud-1-drive-more-storage"),
    ("1441659687", "vk-messenger-live-chat-calls"),
    ("1343242452", "dzen-video-roliki-stat-i"),
    ("6449036810", "vk-znakomstva-obshenie-chat"),
    ("1467719381", "marusya-golosovoj-pomoshnik-"),
    ("1563419339", "vk-mail-secure-email-client"),
    ("1615733452", "vk-clips-short-videos"),
    ("1459096573", "vk-workspace-dlya-biznesa"),
    ("1513171161", "vk-play-app"),
]


def http(url: str, data=None, accept: str = "application/json, text/plain, */*") -> bytes:
    headers = {
        "accept": accept,
        "authorization": f"Bearer {TOKEN}",
        "content-language": "en",
        "accept-language": "ru,en;q=0.9",
        "origin": "https://appmagic.rocks",
        "referer": "https://appmagic.rocks/",
        "user-agent": UA,
        "cookie": COOKIE,
    }
    if data is not None:
        headers["content-type"] = "application/json"
        body = json.dumps(data).encode()
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")
    else:
        req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req, timeout=40) as resp:
        return resp.read()


def to_300(icon: str) -> str:
    icon = icon.strip()
                          
    for p in (
        "https://images.appmagic.rocks/?uri=",
        "http://images.appmagic.rocks/?uri=",
    ):
        if icon.startswith(p):
            icon = urllib.parse.unquote(icon[len(p) :])
            break
                          
    if "mzstatic.com" not in icon:
        return ""
    icon = re.sub(r"/0x\d+h\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    icon = re.sub(r"/\d+x\d+bb\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    icon = re.sub(r"/\d+x\d+\.(jpg|jpeg|png)$", "/0x300h.png", icon, flags=re.I)
    if not icon.endswith("/0x300h.png"):
                                                        
        icon = re.sub(r"\.(jpg|jpeg|png)$", "", icon, flags=re.I)
        if not icon.endswith("/0x300h.png"):
            if "/image/thumb/" in icon and not icon.endswith(".png"):
                                        
                if not re.search(r"/0x\d+h\.png$", icon):
                    icon = icon.rstrip("/") + "/0x300h.png"
    return icon


def extract_mzstatic(text: str) -> list[str]:
                                     
    found = re.findall(
        r"https://is\d+-ssl\.mzstatic\.com/image/thumb/[^\"'\\s<>]+",
        text,
    )
    out = []
    for u in found:
        u = u.rstrip("\\").rstrip("'\"")
        fixed = to_300(u)
        if fixed and fixed not in out:
            out.append(fixed)
    return out


def from_itunes(app_id: str) -> str:
    for country in ("ru", "us", "kz", "by", "am", "uz", "tj", "kg", "md", "az"):
        try:
            url = f"https://itunes.apple.com/lookup?id={app_id}&country={country}"
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=20) as resp:
                data = json.loads(resp.read().decode())
            if data.get("resultCount", 0) > 0:
                x = data["results"][0]
                art = x.get("artworkUrl512") or x.get("artworkUrl100") or ""
                if art:
                                                 
                    if "/512x512bb." in art:
                                                                                     
                                                                         
                        pass
                    art100 = x.get("artworkUrl100") or art
                    if "mzstatic.com" in art100:
                                                                                    
                        fixed = re.sub(
                            r"/\d+x\d+bb\.(jpg|jpeg|png)$",
                            "/0x300h.png",
                            art100,
                            flags=re.I,
                        )
                        if "0x300h.png" in fixed:
                            return fixed
                        return to_300(art100)
        except Exception:
            pass
    return ""


def from_appmagic_page(app_id: str, slug: str) -> str:
    urls = [
        f"https://appmagic.rocks/iphone/{slug}/{app_id}",
        f"https://appmagic.rocks/iphone/{slug}/{app_id}?infoCountry=RU",
        f"https://appmagic.rocks/iphone/{slug}/{app_id}?infoCountry=US",
    ]
    for url in urls:
        try:
            raw = http(url, accept="text/html,application/xhtml+xml")
            text = raw.decode("utf-8", "replace")
            icons = extract_mzstatic(text)
            if icons:
                return icons[0]
                                    
            for m in re.finditer(r'<script[^>]*>(\{.*?})</script>', text, re.S):
                chunk = m.group(1)
                if "mzstatic" in chunk:
                    icons = extract_mzstatic(chunk)
                    if icons:
                        return icons[0]
        except Exception as e:
            print(f"  page err {url}: {e}")
    return ""


def from_appmagic_api(app_id: str) -> str:
                                                                  
    endpoints = [
        (
            "POST",
            "https://appmagic.rocks/api/v2/similarity/app-info",
            {"id": str(app_id)},
        ),
        (
            "GET",
            f"https://appmagic.rocks/api/v2/search?name={app_id}&limit=20",
            None,
        ),
        (
            "GET",
            f"https://appmagic.rocks/api/v2/application-info?store_application_id={app_id}&store=2",
            None,
        ),
        (
            "GET",
            f"https://appmagic.rocks/api/v2/store-application/{app_id}",
            None,
        ),
        (
            "GET",
            f"https://appmagic.rocks/api/v2/united-application?store_application_id={app_id}",
            None,
        ),
    ]
    for method, url, body in endpoints:
        try:
            if method == "POST":
                raw = http(url, data=body)
            else:
                raw = http(url)
            text = raw.decode("utf-8", "replace")
            icons = extract_mzstatic(text)
                                                           
            appicons = [i for i in icons if "AppIcon" in i or "Purple" in i]
            if appicons:
                return appicons[0]
            if icons:
                return icons[0]
        except Exception as e:
            print(f"  api err {url}: {e}")
    return ""


def from_appmagic_images_cdn(app_id: str) -> str:
                                                                     
    candidates = [
        f"https://appmagic.rocks/api/v2/icon?store=2&id={app_id}",
        f"https://appmagic.rocks/api/v2/icon/{app_id}",
        f"https://cdn.appmagic.rocks/icons/ios/{app_id}.png",
    ]
    for url in candidates:
        try:
            raw = http(url)
            text = raw.decode("utf-8", "replace")
            icons = extract_mzstatic(text)
            if icons:
                return icons[0]
                                                                                   
        except Exception:
            pass
    return ""


def main() -> None:
    mapping: dict[str, str] = {}
    for app_id, slug in APPS:
        print(f"== {app_id} {slug}")
        icon = ""
        for fn, name in (
            (lambda: from_itunes(app_id), "itunes"),
            (lambda: from_appmagic_page(app_id, slug), "page"),
            (lambda: from_appmagic_api(app_id), "api"),
            (lambda: from_appmagic_images_cdn(app_id), "cdn"),
        ):
            try:
                icon = fn()
            except Exception as e:
                print(f"  {name} exception: {e}")
                icon = ""
            if icon:
                print(f"  OK via {name}: {icon}")
                break
            print(f"  no {name}")
        if not icon:
            print("  FAILED")
        mapping[app_id] = icon
        time.sleep(0.2)

    (ROOT / "vk_ios_icons.json").write_text(
        json.dumps(mapping, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print("WROTE vk_ios_icons.json")

                                                      
    text = COMMON.read_text(encoding="utf-8")
    changed = 0
    for app_id, icon in mapping.items():
        if not icon:
            continue
                                                                          
                                                                                     
                                                                                                                 
                                                                                                         
                                                                          

                                                                                           
                                                         
        pattern = re.compile(
            rf"(('icon_url'\s*=>\s*')([^']+)('[\s\S]{{0,400}}?'app_store_id'\s*=>\s*'{app_id}')|"
            rf"('app_store_id'\s*=>\s*'{app_id}'[\s\S]{{0,400}}?'icon_url'\s*=>\s*')([^']+)('))",
            re.M,
        )

        def repl(m: re.Match) -> str:
            nonlocal changed
            if m.group(1) and m.group(2):
                                          
                old = m.group(3)
                if old != icon:
                    changed += 1
                    return m.group(2) + icon + m.group(4)
                return m.group(0)
                                      
            old = m.group(6)
            if old != icon:
                changed += 1
                return m.group(5) + icon + m.group(7)
            return m.group(0)

        text, n = pattern.subn(repl, text)

                                                                                               
                                
    for app_id, icon in mapping.items():
        if not icon:
            continue
                                                                                     
                                                                                 
        idx = 0
        while True:
            pos = text.find(f"'app_store_id' => '{app_id}'", idx)
            if pos < 0:
                break
            start = max(0, pos - 1200)
            end = min(len(text), pos + 600)
            chunk = text[start:end]

            def fix_chunk_icons(c: str) -> str:
                nonlocal changed

                def r(m: re.Match) -> str:
                    nonlocal changed
                    old = m.group(2)
                    if "images.appmagic" in old or "play-lh.googleusercontent" in old or "mzstatic.com" in old:
                        if old != icon:
                            changed += 1
                            return m.group(1) + icon + m.group(3)
                    return m.group(0)

                return re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", r, c)

            new_chunk = fix_chunk_icons(chunk)
            text = text[:start] + new_chunk + text[end:]
            idx = pos + 50

                                                                                          
    def final_fix(m: re.Match) -> str:
        nonlocal changed
        old = m.group(2)
        new = to_300(old) if "mzstatic.com" in old or "images.appmagic" in old else old
        if "images.appmagic" in old:
                                                                                                         
            unwrapped = old
            for p in ("https://images.appmagic.rocks/?uri=", "http://images.appmagic.rocks/?uri="):
                if unwrapped.startswith(p):
                    unwrapped = urllib.parse.unquote(unwrapped[len(p) :])
            if "mzstatic.com" in unwrapped:
                new = to_300(unwrapped)
            else:
                                                                                                 
                new = unwrapped
        if new != old:
            changed += 1
        return m.group(1) + new + m.group(3)

    text = re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", final_fix, text)

    COMMON.write_text(text, encoding="utf-8")
    print(f"Patched common.php, icon fixes ~{changed}")
    ok = sum(1 for v in mapping.values() if v)
    print(f"Icons found: {ok}/{len(mapping)}")
    for k, v in mapping.items():
        print(f"  {k}: {v or 'MISSING'}")


if __name__ == "__main__":
    main()
