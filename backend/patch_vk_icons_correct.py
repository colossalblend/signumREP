                      
"""Correctly set VK app icons from vk_ios_icons.json without cross-block bleed."""
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
COMMON = ROOT / "signum" / "common.php"
ICONS = json.loads((ROOT / "vk_ios_icons.json").read_text(encoding="utf-8"))

                                                   
APP_ORDER = [
    ("vk", "564177498"),
    ("ok", "398465290"),
    ("yula", "1016489154"),
    ("mailru", "511310430"),
    ("vk-music", "1054372220"),
    ("vk-video", "6447614666"),
    ("mail-cloud", "696551382"),
    ("vk-messenger", "1441659687"),
    ("dzen", "1343242452"),
    ("vk-dating", "6449036810"),
    ("marusya", "1467719381"),
    ("vk-mail", "1563419339"),
    ("vk-clips", "1615733452"),
    ("vk-workspace", "1459096573"),
    ("vk-play", "1513171161"),
]


def main() -> None:
    text = COMMON.read_text(encoding="utf-8")
    changed = 0

    for catalog_id, store_id in APP_ORDER:
        icon = ICONS.get(store_id)
        if not icon:
            print(f"MISSING icon for {catalog_id} {store_id}")
            continue

                                                                
                                                                    
                                   
        start_pat = re.compile(rf"(        '{re.escape(catalog_id)}' => \[)")
        m = start_pat.search(text)
        if not m:
            print(f"BLOCK NOT FOUND {catalog_id}")
            continue
        start = m.start()

                                                                                  
                                                                             
        rest = text[start + 10 :]
        next_entry = re.search(r"\n        '[a-z0-9_-]+' => \[", rest)
        end_catalog = rest.find("\n    ];")
        if next_entry and (end_catalog < 0 or next_entry.start() < end_catalog):
            end = start + 10 + next_entry.start()
        elif end_catalog >= 0:
            end = start + 10 + end_catalog
        else:
            print(f"END NOT FOUND {catalog_id}")
            continue

        block = text[start:end]
                                                 
        def repl(mm: re.Match) -> str:
            nonlocal changed
            old = mm.group(2)
            if old != icon:
                changed += 1
            return mm.group(1) + icon + mm.group(3)

        new_block = re.sub(r"('icon_url'\s*=>\s*')([^']+)(')", repl, block)
        text = text[:start] + new_block + text[end:]
        print(f"OK {catalog_id} -> {icon[:80]}...")

    COMMON.write_text(text, encoding="utf-8")
    print(f"changed icon fields: {changed}")

                                                             
    for catalog_id, store_id in APP_ORDER:
        icon = ICONS[store_id]
                                                                 
        m = re.search(
            rf"'{re.escape(catalog_id)}' => \[(.*?)(\n        '[a-z0-9_-]+' => \[|\n    \];)",
            text,
            re.S,
        )
        if not m:
            continue
        block = m.group(1)
        icons_in_block = re.findall(r"'icon_url'\s*=>\s*'([^']+)'", block)
        bad = [u for u in icons_in_block if u != icon]
        if bad:
            print(f"WARN {catalog_id} still has mismatched icons: {bad}")
        else:
            print(f"VERIFY {catalog_id}: {len(icons_in_block)} icons match")


if __name__ == "__main__":
    main()
