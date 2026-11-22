from __future__ import annotations

import ast
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = r"[\U0001f300-\U0001faff\u2300-\u23ff\u25b6\u25c0\u2600-\u27bf\u2b05-\u2b07\u2b1b\u2b1c\u2b50\u2b55]"
PATTERN = re.compile(
    r"[0-9#*]\ufe0f?\u20e3|[\U0001f1e6-\U0001f1ff]{2}|"
    + BASE
    + r"[\ufe0e\ufe0f]?(?:\u200d"
    + BASE
    + r"[\ufe0e\ufe0f]?)*"
)


def inventory() -> list[str]:
    found = set()
    for path in (ROOT / "signum_bot").glob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            if isinstance(node, ast.Constant) and isinstance(node.value, str):
                if node.value.startswith("[") and node.value.endswith("]"):
                    continue
                found.update(PATTERN.findall(node.value))
    selection = json.loads(
        (ROOT / "signum_bot/assets/button-emojis.json").read_text(encoding="utf-8")
    )
    found.update(selection.get("text", {}))
    found.update(selection.get("fallbacks", {}).values())
    found.update((str(digit) + "\ufe0f\u20e3") for digit in range(10))
    found.update(("🗂", "🔍", "ℹ️", "🏠", "📄", "⬇️"))
    normalized = {}
    for emoji in sorted(found):
        plain = emoji.replace("\ufe0e", "").replace("\ufe0f", "")
        normalized[plain] = emoji
    return sorted(normalized.values())


def main():
    emojis = inventory()
    path = ROOT / "docs/bot-emojis.txt"
    path.parent.mkdir(exist_ok=True)
    rows = [" ".join(emojis[start : start + 12]) for start in range(0, len(emojis), 12)]
    path.write_text(
        "Все обычные эмодзи бота — без ID и без анимации\n\n"
        + "\n".join(rows)
        + "\n\nЗначки навигации и оформления: ▦ ⌕ ⌂ ◷ ▰ ▱ → ↓ ↑\n",
        encoding="utf-8",
    )
    print(
        json.dumps(
            {"count": len(emojis), "emojis": emojis, "file": str(path)},
            ensure_ascii=True,
        )
    )


if __name__ == "__main__":
    main()
