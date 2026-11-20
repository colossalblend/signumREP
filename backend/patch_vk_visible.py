                      
import re
from pathlib import Path

path = Path(__file__).resolve().parent / "signum" / "common.php"
text = path.read_text(encoding="utf-8")
ids = [
    "vk",
    "ok",
    "yula",
    "mailru",
    "vk-music",
    "vk-video",
    "mail-cloud",
    "vk-messenger",
    "dzen",
    "vk-dating",
    "marusya",
    "vk-mail",
    "vk-clips",
    "vk-workspace",
    "vk-play",
]
changed = 0
for cid in ids:
    pat = re.compile(
        rf"(        '{re.escape(cid)}' => \[\n"
        rf"            'id' => '{re.escape(cid)}',\n"
        rf"            'active' => true,\n"
        rf"            'in_store' => (?:true|false),)\n"
    )
    m = pat.search(text)
    if not m:
        print("NOT FOUND", cid)
        continue
    window = text[m.end() : m.end() + 220]
    if "visible_for_versions" in window:
        print("already", cid)
        continue
    insert = (
        m.group(1)
        + "\n"
        + "            // Test-only: hide from everyone except desktop client 1.0.3\n"
        + "            'visible_for_versions' => ['1.0.3'],\n"
    )
    text = text[: m.start()] + insert + text[m.end() :]
    changed += 1
    print("OK", cid)

path.write_text(text, encoding="utf-8")
print("changed", changed)
