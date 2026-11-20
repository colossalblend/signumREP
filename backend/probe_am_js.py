                      
"""Download AppMagic main JS and extract API endpoints related to icons/apps."""
import re
import urllib.request

UA = "Mozilla/5.0"
urls = [
    "https://appmagic.rocks/assets/angular2/main-2SYREXE4.js",
    "https://appmagic.rocks/assets/angular2/main.js",
]

for url in urls:
    print("GET", url)
    try:
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=60) as r:
            body = r.read().decode("utf-8", "replace", errors="replace")
        print("len", len(body))
                        
        apis = sorted(set(re.findall(r"/api/v2/[a-zA-Z0-9_./?-]+", body)))
        for a in apis:
            if any(
                k in a.lower()
                for k in (
                    "icon",
                    "app",
                    "store",
                    "info",
                    "meta",
                    "search",
                    "united",
                    "simil",
                    "base",
                    "detail",
                )
            ):
                print(" ", a)
                           
        for m in re.findall(r".{0,40}icon_url.{0,40}", body)[:20]:
            print("ICONCTX", m[:120])
        open(r"D:\Signum-v1\backend\am_main.js", "w", encoding="utf-8").write(body[:500000])
        print("saved head of main js")
        break
    except Exception as e:
        print("fail", e)
