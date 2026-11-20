                      
"""Generate VK catalog PHP block from vk_apps_lookup.json and insert into common.php."""
from __future__ import annotations

import re
from pathlib import Path
from urllib.parse import quote
import json

ROOT = Path(__file__).resolve().parent
COMMON = ROOT / "signum" / "common.php"
LOOKUP = ROOT / "vk_apps_lookup.json"

IOS_BUNDLES = {
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

CATALOG_IDS = {
    "vk": "vk",
    "ok": "ok",
    "yula": "yula",
    "mail": "mailru",
    "vkmusic": "vk-music",
    "vkvideo": "vk-video",
    "cloud": "mail-cloud",
    "vkmessenger": "vk-messenger",
    "dzen": "dzen",
    "vkdating": "vk-dating",
    "marusya": "marusya",
    "vkmail": "vk-mail",
    "vkclips": "vk-clips",
    "vkws": "vk-workspace",
    "vkplay": "vk-play",
}

ALIASES = {
    "vk": ["VK", "ВК", "VKontakte", "Вконтакте", "VK App"],
    "ok": ["OK", "OK.ru", "Odnoklassniki"],
    "yula": ["Youla", "Юла объявления", "Yula"],
    "mail": ["Mail.ru", "Mail", "Почта Mail", "MailRU"],
    "vkmusic": ["VK Music", "VK Музыка", "ВКонтакте Музыка"],
    "vkvideo": ["VK Video", "VK Видео", "ВКонтакте Видео"],
    "cloud": ["Cloud Mail", "Mail Cloud", "Облако Mail", "Mail.ru Cloud"],
    "vkmessenger": ["VK Messenger", "VK IM"],
    "dzen": ["Dzen", "Yandex Zen", "Яндекс Дзен", "Zen"],
    "vkdating": ["VK Dating", "VK Знакомства"],
    "marusya": ["Marusya", "Mail Marusya"],
    "vkmail": ["VK Mail", "ВК Почта"],
    "vkclips": ["VK Clips", "ВК Клипы"],
    "vkws": ["VK Workspace", "VK Teams", "VK WorkSpace"],
    "vkplay": ["VK Play", "ВК Плей", "VKPlay"],
}


def php_str(s: str) -> str:
    return "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"


def pick_bundle(app: dict) -> str:
    sid = str(app["id"])
    am = (app.get("bundle_id") or "").strip()
                                                    
    if am.startswith("com.vk.") and am != "com.vkontakte.android":
        return am
    if am in {
        "ru.mail.cloud",
        "ru.mail.search.electroscope",
        "ru.mail.biz.avocado",
    }:
        return am
    return IOS_BUNDLES.get(sid, am or "unknown")


def pick_icon(app: dict) -> str:
    icon = (app.get("icon") or "").strip()
    raw = (app.get("raw_icon") or "").strip()
    if icon and "mzstatic.com" in icon:
        if "/0x" in icon:
            return icon.split("/0x")[0] + "/0x300h.png"
        return icon
    if icon and "images.appmagic.rocks" in icon:
        return icon
    src = icon or raw
    if not src:
        return ""
    if "mzstatic.com" in src:
        if "/0x" in src:
            return src.split("/0x")[0] + "/0x300h.png"
        return src
    return "https://images.appmagic.rocks/?uri=" + quote(src, safe="")


def render_block(apps: list[dict]) -> str:
    chunks: list[str] = []
    base_priority = 75
    for i, app in enumerate(apps):
        key = app["key"]
        cid = CATALOG_IDS[key]
        title = app["title"]
        sid = str(app["id"])
        icon = pick_icon(app)
        bundle = pick_bundle(app)
        pr = base_priority - i
        aliases = ALIASES.get(key, [])
        aliases_php = ", ".join(php_str(a) for a in aliases)
        chunks.append(
            f"""        {php_str(cid)} => [
            'id' => {php_str(cid)},
            'active' => true,
            'in_store' => false,
            'title' => {php_str(title)},
            'aliases' => [{aliases_php}],
            'app_store_url' => null,
            'color' => '#0077FF',
            'icon_url' => {php_str(icon)},
            'priority' => {pr},
            'category' => 'vk',
            'versions' => [
                [
                    'title' => {php_str(title)},
                    'description' => '',
                    'version' => '',
                    'bundle_id' => {php_str(bundle)},
                    'app_store_id' => {php_str(sid)},
                    'icon_url' => {php_str(icon)},
                    'min_ios' => '15.0',
                    'is_latest' => true,
                    'source' => 'removed',
                    'in_store' => false,
                ],
            ],
        ],"""
        )
    return "\n".join(chunks)


def main() -> None:
    apps = json.loads(LOOKUP.read_text(encoding="utf-8"))
    block = render_block(apps)

    text = COMMON.read_text(encoding="utf-8")

                      
    if "'id' => 'vk'" not in text and "['id' => 'vk'" not in text:
        text = text.replace(
            "        ['id' => 'yoomoney', 'title' => 'ЮMoney', 'priority' => 40],\n",
            "        ['id' => 'vk', 'title' => 'ВК', 'priority' => 75],\n"
            "        ['id' => 'yoomoney', 'title' => 'ЮMoney', 'priority' => 40],\n",
        )

                                           
    marker = "        ],\n    ];\n\n    // Добавляем active true/false к каждой версии."
                                                      
                                                                                        
                                                                         
    insert_anchor = None
    for anchor in (
        "            ],\n        ],\n    ];\n\n    // Добавляем active true/false к каждой версии.",
    ):
        if anchor in text:
            insert_anchor = anchor
            break
    if not insert_anchor:
        raise SystemExit("Could not find catalog end anchor")

                         
    if "'id' => 'vk'" in text and "'category' => 'vk'" in text:
                                                                                        
                                                                        
        print("VK entries may already exist; rewriting managed keys if present")

    replacement = (
        "            ],\n"
        "        ],\n"
        f"{block}\n"
        "    ];\n\n"
        "    // Добавляем active true/false к каждой версии."
    )
    if text.count(insert_anchor) != 1:
                                               
        zenit_tail = "'app_store_id' => '1364829235'"
        idx = text.find(zenit_tail)
        if idx < 0:
            raise SystemExit(f"anchor count={text.count(insert_anchor)}, no zenit either")
                                       
        end = text.find("    ];\n\n    // Добавляем active true/false", idx)
        if end < 0:
            raise SystemExit("zenit found but catalog end not found after it")
                                                                                          
                                                             
                                
        text = text[:end] + block + "\n" + text[end:]
    else:
        text = text.replace(insert_anchor, replacement, 1)

    COMMON.write_text(text, encoding="utf-8")
    print(f"Updated {COMMON} with {len(apps)} VK apps")


if __name__ == "__main__":
    main()
