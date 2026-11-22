from pathlib import Path


def main():
    root = Path(__file__).resolve().parent.parent
    target = root / "apple_bridge/src/vendor"
    target.mkdir(parents=True, exist_ok=True)
    for name in ("apple_history", "apple_region"):
        source = (root.parent / "src-tauri/src" / f"{name}.rs").read_text(
            encoding="utf-8"
        )
        if name == "apple_history":
            start = source.index("    fn write_raw(")
            end = source.index("    fn note(", start)
            source = (
                source[:start]
                + "    fn write_raw(&self, _text: &str) {}\n\n"
                + source[end:]
            )
            start = source.index("pub async fn fetch_history_via_ipatool(")
            end = source.index("\n#[cfg(test)]", start)
            source = source[:start] + source[end:]
        source = (
            "\n".join(line.rstrip() for line in source.splitlines() if line.strip())
            + "\n"
        )
        (target / f"{name}.rs").write_text(source, encoding="utf-8")
    print("Signum Apple modules copied; HTTP debug output disabled.")


if __name__ == "__main__":
    main()
