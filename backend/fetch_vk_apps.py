                      
"""Fetch VK ecosystem apps from AppMagic (auth from New folder/request.txt)."""
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

TOKEN = "S5tgAvA8yFGByYWyXBoc-7Wv4YpQ29kE"
UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/148.0.0.0 YaBrowser/26.6.0.0 Safari/537.36"
)
COOKIE = (
    "dashly_device_guid=161f5bb5-a711-4bbe-a7fb-e0c5bddc85fd; "
    "dashly_uid=2066609493382267361; "
    "dashly_auth_token=user.2066609493382267361.7830-e4dd41f4662c0285e8163b41d6e.7d1895f9543ec78c882e2ad8693967b36a4225c692956c09; "
    "hubspotutk=5ace068022dd152547b0568f1b78b3ec; "
    "cf_clearance=QAwXyufztJ6L6vdKuW6zNiDaOPpkvUbhbbQe._UGM64-1771183652-1.2.1.1-Gzhj7Nw3tXtEkKBttkwxHlYuenFnNRq18zMYghCiPUSw7zbz5CDd.gr9CTnBx6rtAPmiT.VVoAiQKLLINiof3HNAbIMqRdCYDuUE390Hi9oGg9IYufYFoYK.92_1vmd19rUo3bv_QOjqazslCxwZYN4O2YsAZabjeV4fOEtI7T_DTh8fzGE03aN9Vv4ErVyJo.zYlxPietM1dooVOqArA7ZzzcF7MHtkc5oRHfvXjgw"
)

APPS = [
    ("vk", "ВКонтакте", "564177498", "https://appmagic.rocks/iphone/vk-social-network-messenger/564177498"),
    ("ok", "Одноклассники", "398465290", "https://appmagic.rocks/iphone/odnoklassniki-social-network/398465290"),
    ("yula", "Юла", "1016489154", "https://appmagic.rocks/iphone/yula-obuyavleniya-ryadom/1016489154"),
    ("mail", "Почта Mail.RU", "511310430", "https://appmagic.rocks/iphone/mail-email-cloud-calendar/511310430"),
    ("vkmusic", "ВК Музыка", "1054372220", "https://appmagic.rocks/iphone/vk-music-playlists-and-podcasts/1054372220"),
    ("vkvideo", "ВК Видео", "6447614666", "https://appmagic.rocks/iphone/vk-video-shows-films-series/6447614666"),
    ("cloud", "Облако Mail.ru", "696551382", "https://appmagic.rocks/iphone/cloud-1-drive-more-storage/696551382"),
    ("vkmessenger", "ВК Мессенджер", "1441659687", "https://appmagic.rocks/iphone/vk-messenger-live-chat-calls/1441659687"),
    ("dzen", "Дзен", "1343242452", "https://appmagic.rocks/iphone/dzen-video-roliki-stat-i/1343242452"),
    ("vkdating", "ВК Знакомства", "6449036810", "https://appmagic.rocks/iphone/vk-znakomstva-obshenie-chat/6449036810"),
    ("marusya", "Маруся", "1467719381", "https://appmagic.rocks/iphone/marusya-golosovoj-pomoshnik-/1467719381"),
    ("vkmail", "VK Mail", "1563419339", "https://appmagic.rocks/iphone/vk-mail-secure-email-client/1563419339"),
    ("vkclips", "ВК Клипы", "1615733452", "https://appmagic.rocks/iphone/vk-clips-short-videos/1615733452"),
    ("vkws", "VK Workspace", "1459096573", "https://appmagic.rocks/iphone/vk-workspace-dlya-biznesa/1459096573"),
    ("vkplay", "VK Play", "1513171161", "https://appmagic.rocks/iphone/vk-play-app/1513171161"),
]

                                                                   
KNOWN_BUNDLES = {
    "564177498": "com.vk.vkclient",
    "398465290": "ru.odnoklassniki.iphone",
    "1016489154": "ru.youla",
    "511310430": "ru.mail.mail",
    "1054372220": "com.vk.vkclient.music",
    "6447614666": "com.vk.vkvideo",
    "696551382": "ru.mail.cloud",
    "1441659687": "com.vk.im",
    "1343242452": "ru.yandex.zen",
    "6449036810": "com.vk.love",
    "1467719381": "ru.mail.search.electroscope",
    "1563419339": "com.vk.mail",
    "1615733452": "com.vk.clips",
    "1459096573": "ru.mail.biz.avocado",
    "1513171161": "com.vk.play",
}


def api(url: str, data=None):
    headers = {
        "accept": "application/json, text/plain, */*",
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
        return json.loads(resp.read().decode("utf-8", "replace"))


def icon300(icon: str | None) -> str | None:
    if not icon:
        return None
    if "mzstatic.com" in icon and "/0x" in icon:
        return icon.split("/0x")[0] + "/0x300h.png"
    return icon


def pick_bundle(store_ids: list) -> str | None:
    for s in store_ids or []:
        s = str(s)
        if s.isdigit():
            continue
                                                                                  
        if s.startswith(("com.", "ru.", "app.", "net.", "org.", "io.")):
            return s
    return None


def itunes_lookup(app_id: str) -> dict:
    for country in ("ru", "us", "kz", "by", "am", "uz", "tj"):
        try:
            url = f"https://itunes.apple.com/lookup?id={app_id}&country={country}"
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=20) as resp:
                data = json.loads(resp.read().decode())
            if data.get("resultCount", 0) > 0:
                x = data["results"][0]
                art = x.get("artworkUrl512") or x.get("artworkUrl100") or ""
                if "mzstatic.com" in art:
                                                                                     
                    if "/512x512bb." in art:
                        art = art.rsplit("/", 1)[0] + "/0x300h.png"
                    elif art.endswith("bb.jpg") or art.endswith("bb.png"):
                        art = art.rsplit("/", 1)[0] + "/0x300h.png"
                return {
                    "in_store": True,
                    "country": country,
                    "name": x.get("trackName"),
                    "version": x.get("version"),
                    "bundle_id": x.get("bundleId"),
                    "min_ios": x.get("minimumOsVersion"),
                    "icon": art,
                }
        except Exception:
            pass
    return {"in_store": False}


def search_app(store_id: str, title: str) -> dict | None:
    for q in (store_id, title):
        try:
            url = (
                "https://appmagic.rocks/api/v2/search?name="
                + urllib.parse.quote(q)
                + "&limit=20"
            )
            data = api(url)
            for app in data.get("applications", []):
                sids = [str(x) for x in app.get("store_application_ids", [])]
                if store_id in sids:
                    return app
        except Exception as e:
            print(f"  search err {store_id} q={q}: {e}")
    return None


def main():
    out = []
    for key, title, sid, page in APPS:
        print(f"== {sid} {title}")
        found = search_app(sid, title)
        row = {
            "key": key,
            "title": title,
            "id": sid,
            "page": page,
        }
        if found:
            icon = icon300(found.get("icon_url"))
                                                                             
            if icon and "mzstatic.com" not in icon and "play-lh.googleusercontent" in icon:
                icon = "https://images.appmagic.rocks/?uri=" + urllib.parse.quote(icon, safe="")
            sids = [str(x) for x in found.get("store_application_ids", [])]
            bundle = pick_bundle(sids) or KNOWN_BUNDLES.get(sid)
            row.update(
                {
                    "am_name": found.get("name"),
                    "icon": icon,
                    "raw_icon": found.get("icon_url"),
                    "removed": bool(found.get("removed")),
                    "sids": sids,
                    "bundle_id": bundle,
                    "last_release": found.get("last_release_date") or found.get("releaseDate"),
                    "store_ids": found.get("store_ids"),
                    "apps_priority": found.get("apps_priority"),
                }
            )
        else:
            row["am_name"] = title
            row["bundle_id"] = KNOWN_BUNDLES.get(sid)
            row["removed"] = True
            row["icon"] = None

        it = itunes_lookup(sid)
        row["itunes"] = it
        if it.get("in_store"):
            row["version"] = it.get("version")
            row["min_ios"] = it.get("min_ios")
            row["bundle_id"] = it.get("bundle_id") or row.get("bundle_id")
            if it.get("icon") and "mzstatic.com" in str(it.get("icon")):
                row["icon"] = it["icon"]
            row["in_store"] = True
        else:
            row["in_store"] = False
            row["version"] = row.get("version") or ""
            row["min_ios"] = "15.0"

        print(
            f"  name={row.get('am_name')} in_store={row.get('in_store')} "
            f"bundle={row.get('bundle_id')} icon={str(row.get('icon'))[:100]}"
        )
        out.append(row)
        time.sleep(0.15)

    path = Path(__file__).with_name("vk_apps_lookup.json")
    path.write_text(json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    print("WROTE", path, "count=", len(out))


if __name__ == "__main__":
    main()
