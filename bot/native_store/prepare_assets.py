from pathlib import Path
import argparse
import hashlib
import json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    assets = json.loads((root / "assets.json").read_text())
    output = root / "sap_assets"
    with args.binary.open("rb") as source:
        verified = []
        for asset in assets:
            source.seek(asset["offset"])
            data = source.read(asset["size"])
            if hashlib.sha256(data).hexdigest() != asset["sha256"]:
                raise SystemExit(f"Asset mismatch: {asset['name']}")
            verified.append((asset["name"], data))
    output.mkdir(exist_ok=True)
    for name, data in verified:
        (output / name).write_bytes(data)
    print(f"Verified {len(verified)} assets")


if __name__ == "__main__":
    main()
