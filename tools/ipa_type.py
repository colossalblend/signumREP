                      
"""Определяет тип IPA: зашифрован под Apple ID, декрипт или подписан.

Без внешних зависимостей. Смотрит Mach-O cryptid (FairPlay),
подпись (LC_CODE_SIGNATURE + CMS) и iTunesMetadata / SC_Info.
"""

from __future__ import annotations

import argparse
import json
import plistlib
import struct
import sys
import zipfile
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Iterable

MH_MAGIC = 0xFEEDFACE
MH_CIGAM = 0xCEFAEDFE
MH_MAGIC_64 = 0xFEEDFACF
MH_CIGAM_64 = 0xCFFAEDFE
FAT_MAGIC = 0xCAFEBABE
FAT_CIGAM = 0xBEBAFECA
FAT_MAGIC_64 = 0xCAFEBABF
FAT_CIGAM_64 = 0xBFBAFECA

LC_CODE_SIGNATURE = 0x1D
LC_ENCRYPTION_INFO = 0x21
LC_ENCRYPTION_INFO_64 = 0x2C

CSMAGIC_EMBEDDED_SIGNATURE = 0xFADE0CC0
CSMAGIC_CODEDIRECTORY = 0xFADE0C02
CSMAGIC_BLOBWRAPPER = 0xFADE0B01
CSSLOT_CODEDIRECTORY = 0
CSSLOT_CMS_SIGNATURE = 0x10000

CPU = {
    7: "x86",
    0x01000007: "x86_64",
    12: "arm",
    0x0100000C: "arm64",
    0x0200000C: "arm64_32",
}

STORE_SIGNERS = (
    "apple iphone os application signing",
    "apple iphone certification authority",
    "apple itunes store",
)
DEV_SIGNERS = (
    "iphone developer:",
    "apple development:",
    "iphone os application development",
)
DIST_SIGNERS = (
    "iphone distribution:",
    "apple distribution:",
)
ENTERPRISE_HINTS = (
    "in-house",
    "enterprise",
)


@dataclass
class SliceInfo:
    cpu: str
    cryptid: int | None = None
    cryptoff: int | None = None
    cryptsize: int | None = None
    signed: bool = False
    team_id: str | None = None
    signer: str | None = None
    signature_kind: str | None = None


@dataclass
class IpaReport:
    path: str
    ok: bool
    type: str
    type_label: str
    encrypted: bool
    decrypted: bool
    signed: bool
    apple_id: str | None = None
    adam_id: str | None = None
    bundle_id: str | None = None
    name: str | None = None
    version: str | None = None
    executable: str | None = None
    has_sinf: bool = False
    has_itunes_metadata: bool = False
    signature_kind: str | None = None
    team_id: str | None = None
    signer: str | None = None
    slices: list[dict[str, Any]] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    error: str | None = None


def _u32(data: bytes, off: int, be: bool) -> int:
    if off + 4 > len(data):
        raise ValueError("обрезанный Mach-O")
    fmt = ">I" if be else "<I"
    return struct.unpack_from(fmt, data, off)[0]


def _u64(data: bytes, off: int, be: bool) -> int:
    if off + 8 > len(data):
        raise ValueError("обрезанный Mach-O")
    fmt = ">Q" if be else "<Q"
    return struct.unpack_from(fmt, data, off)[0]


def _looks_macho(magic: int) -> bool:
    return magic in (MH_MAGIC, MH_CIGAM, MH_MAGIC_64, MH_CIGAM_64)


def _looks_fat(magic: int) -> bool:
    return magic in (FAT_MAGIC, FAT_CIGAM, FAT_MAGIC_64, FAT_CIGAM_64)


def _cpu_name(cputype: int) -> str:
    return CPU.get(cputype & 0xFFFFFFFF, f"cpu_0x{cputype & 0xFFFFFFFF:x}")


def _parse_code_directory(blob: bytes) -> tuple[str | None, str | None]:
    if len(blob) < 44 or _u32(blob, 0, True) != CSMAGIC_CODEDIRECTORY:
        return None, None
    version = _u32(blob, 8, True)
    ident_off = _u32(blob, 20, True)
    ident = _cstr(blob, ident_off) if ident_off else None
    team = None
    if version >= 0x20200 and len(blob) >= 52:
        team_off = _u32(blob, 48, True)
        if team_off:
            team = _cstr(blob, team_off)
    return ident, team


def _cstr(data: bytes, off: int) -> str | None:
    if off <= 0 or off >= len(data):
        return None
    end = data.find(b"\x00", off)
    if end < 0:
        end = len(data)
    raw = data[off:end]
    if not raw:
        return None
    return raw.decode("utf-8", errors="replace")


def _signer_from_cms(cms: bytes) -> str | None:
    text = cms.decode("latin-1", errors="ignore")
    markers = (
        "Apple iPhone OS Application Signing",
        "iPhone Distribution:",
        "iPhone Developer:",
        "Apple Development:",
        "Apple Distribution:",
        "iPhone OS Application Development",
    )
    for marker in markers:
        idx = text.find(marker)
        if idx < 0:
            continue
        chunk = text[idx : idx + 160]
        cleaned = []
        for ch in chunk:
            if ch == "\x00":
                break
            if 32 <= ord(ch) < 127:
                cleaned.append(ch)
            elif cleaned and cleaned[-1] != " ":
                cleaned.append(" ")
        label = "".join(cleaned).strip(" .")
        if label:
            return label[:120]
    return None


def _signature_kind(signer: str | None) -> str | None:
    if not signer:
        return "present"
    low = signer.lower()
    if any(s in low for s in STORE_SIGNERS):
        return "app_store"
    if any(s in low for s in ENTERPRISE_HINTS):
        return "enterprise"
    if any(s in low for s in DEV_SIGNERS):
        return "development"
    if any(s in low for s in DIST_SIGNERS):
        return "distribution"
    return "present"


def _parse_superblob(blob: bytes) -> tuple[str | None, str | None, str | None]:
    if len(blob) < 12 or _u32(blob, 0, True) != CSMAGIC_EMBEDDED_SIGNATURE:
        return None, None, None
    count = _u32(blob, 8, True)
    team = None
    ident = None
    signer = None
    for i in range(count):
        base = 12 + i * 8
        if base + 8 > len(blob):
            break
        slot = _u32(blob, base, True)
        off = _u32(blob, base + 4, True)
        if off + 8 > len(blob):
            continue
        magic = _u32(blob, off, True)
        length = _u32(blob, off + 4, True)
        piece = blob[off : off + length]
        if slot == CSSLOT_CODEDIRECTORY or magic == CSMAGIC_CODEDIRECTORY:
            ident, team = _parse_code_directory(piece)
        elif slot == CSSLOT_CMS_SIGNATURE or magic == CSMAGIC_BLOBWRAPPER:
            signer = _signer_from_cms(piece)
    return ident, team, signer


def parse_macho_slice(data: bytes) -> SliceInfo:
    if len(data) < 32:
        raise ValueError("слишком короткий Mach-O")
    magic = _u32(data, 0, True)
    if magic in (MH_CIGAM, MH_CIGAM_64, MH_MAGIC, MH_MAGIC_64):
        be = magic in (MH_MAGIC, MH_MAGIC_64)
    else:
        magic = _u32(data, 0, False)
        be = False
        if not _looks_macho(magic):
            raise ValueError("не Mach-O")
    is64 = magic in (MH_MAGIC_64, MH_CIGAM_64)
    cputype = _u32(data, 4, be)
    ncmds = _u32(data, 16, be)
    sizeofcmds = _u32(data, 20, be)
    header = 32 if is64 else 28
    info = SliceInfo(cpu=_cpu_name(cputype))
    off = header
    end = min(len(data), header + sizeofcmds)
    for _ in range(ncmds):
        if off + 8 > end:
            break
        cmd = _u32(data, off, be)
        cmdsize = _u32(data, off + 4, be)
        if cmdsize < 8 or off + cmdsize > len(data):
            break
        if cmd in (LC_ENCRYPTION_INFO, LC_ENCRYPTION_INFO_64) and cmdsize >= 20:
            info.cryptoff = _u32(data, off + 8, be)
            info.cryptsize = _u32(data, off + 12, be)
            info.cryptid = _u32(data, off + 16, be)
        elif cmd == LC_CODE_SIGNATURE and cmdsize >= 16:
            dataoff = _u32(data, off + 8, be)
            datasize = _u32(data, off + 12, be)
            info.signed = True
            if dataoff >= 0 and datasize > 12 and dataoff + datasize <= len(data):
                _ident, team, signer = _parse_superblob(data[dataoff : dataoff + datasize])
                info.team_id = team
                info.signer = signer
                info.signature_kind = _signature_kind(signer) if signer else "present"
        off += cmdsize
    return info


def parse_macho(data: bytes) -> list[SliceInfo]:
    if len(data) < 8:
        raise ValueError("пустой бинарник")
    magic = _u32(data, 0, True)
    if _looks_macho(magic) or _looks_macho(_u32(data, 0, False)):
        return [parse_macho_slice(data)]
    if not _looks_fat(magic):
        raise ValueError("не Mach-O и не FAT")
    be = magic in (FAT_MAGIC, FAT_MAGIC_64)
    is64 = magic in (FAT_MAGIC_64, FAT_CIGAM_64)
    narch = _u32(data, 4, be)
    slices: list[SliceInfo] = []
    rec = 32 if is64 else 20
    for i in range(min(narch, 16)):
        base = 8 + i * rec
        if base + rec > len(data):
            break
        if is64:
            offset = _u64(data, base + 8, be)
            size = _u64(data, base + 16, be)
        else:
            offset = _u32(data, base + 8, be)
            size = _u32(data, base + 12, be)
        if offset + 32 > len(data):
            continue
        chunk = data[offset : offset + size] if size else data[offset:]
        try:
            slices.append(parse_macho_slice(chunk))
        except ValueError:
            continue
    if not slices:
        raise ValueError("FAT без рабочих срезов")
    return slices


def _app_plist_name(names: Iterable[str]) -> str | None:
    apps = [
        n
        for n in names
        if n.startswith("Payload/") and n.endswith(".app/Info.plist") and n.count("/") == 2
    ]
    if not apps:
        apps = [
            n
            for n in names
            if n.startswith("Payload/") and n.endswith(".app/Info.plist")
        ]
    return min(apps, key=len) if apps else None


def _load_plist(raw: bytes) -> dict[str, Any]:
    try:
        data = plistlib.loads(raw)
    except Exception:
        return {}
    return data if isinstance(data, dict) else {}


def _meta_apple_id(meta: dict[str, Any]) -> str | None:
    for key in ("appleId", "appleID", "userName", "AppleID"):
        value = meta.get(key)
        if isinstance(value, str) and "@" in value:
            return value.strip()
    download = meta.get("com.apple.iTunesStore.downloadInfo")
    if isinstance(download, dict):
        account = download.get("accountInfo")
        if isinstance(account, dict):
            for key in ("AppleID", "appleId", "AccountHint"):
                value = account.get(key)
                if isinstance(value, str) and value.strip():
                    return value.strip()
    return None


def _meta_adam(meta: dict[str, Any]) -> str | None:
    for key in ("itemId", "softwareVersionBundleId", "playlistItemId"):
        value = meta.get(key)
        if isinstance(value, (int, str)) and str(value).isdigit():
            return str(value)
    return None


def classify_ipa(path: Path) -> IpaReport:
    report = IpaReport(
        path=str(path),
        ok=False,
        type="invalid",
        type_label="не IPA",
        encrypted=False,
        decrypted=False,
        signed=False,
    )
    if not path.is_file():
        report.error = "файл не найден"
        return report
    try:
        zf = zipfile.ZipFile(path)
    except zipfile.BadZipFile:
        report.error = "это не zip/ipa"
        return report
    with zf:
        names = zf.namelist()
        plist_name = _app_plist_name(names)
        if not plist_name:
            report.error = "нет Payload/*.app/Info.plist"
            return report
        info = _load_plist(zf.read(plist_name))
        report.bundle_id = str(info.get("CFBundleIdentifier") or "") or None
        report.name = str(info.get("CFBundleDisplayName") or info.get("CFBundleName") or "") or None
        report.version = str(info.get("CFBundleShortVersionString") or info.get("CFBundleVersion") or "") or None
        exe = str(info.get("CFBundleExecutable") or "").strip()
        report.executable = exe or None
        app_dir = plist_name[: -len("Info.plist")]
        exe_name = f"{app_dir}{exe}" if exe else None
        if exe_name and exe_name not in names:
            exe_name = next(
                (n for n in names if n.startswith(app_dir) and n.rstrip("/").endswith("/" + exe)),
                None,
            )
        report.has_sinf = any("/SC_Info/" in n and n.endswith(".sinf") for n in names)
        meta_name = next(
            (n for n in names if n == "iTunesMetadata.plist" or n.endswith("/iTunesMetadata.plist")),
            None,
        )
        if meta_name:
            report.has_itunes_metadata = True
            meta = _load_plist(zf.read(meta_name))
            report.apple_id = _meta_apple_id(meta)
            report.adam_id = _meta_adam(meta)
        if not exe_name:
            report.error = "в Info.plist нет исполняемого файла"
            return report
        raw = zf.read(exe_name)
        try:
            slices = parse_macho(raw)
        except ValueError as exc:
            report.error = f"бинарник: {exc}"
            return report

    report.slices = [asdict(s) for s in slices]
    cryptids = [s.cryptid for s in slices if s.cryptid is not None]
    report.encrypted = any(c == 1 for c in cryptids)
    report.decrypted = bool(cryptids) and all(c == 0 for c in cryptids)
    if not cryptids:
        report.decrypted = True
        report.notes.append("LC_ENCRYPTION_INFO нет — FairPlay на бинарнике не стоит")
    report.signed = any(s.signed for s in slices)
    signed_slice = next((s for s in slices if s.signed), None)
    if signed_slice:
        report.team_id = signed_slice.team_id
        report.signer = signed_slice.signer
        report.signature_kind = signed_slice.signature_kind
    if report.encrypted:
        report.type = "encrypted_apple_id"
        report.type_label = "зашифрован под Apple ID"
        if report.apple_id:
            report.notes.append(f"аккаунт в метаданных: {report.apple_id}")
        elif report.has_sinf or report.has_itunes_metadata:
            report.notes.append("FairPlay cryptid=1, типичный магазинный IPA")
        else:
            report.notes.append("FairPlay cryptid=1 — нужен Apple ID / покупка")
    elif report.signed and report.signature_kind == "development":
        report.type = "signed"
        report.type_label = "подписан"
        report.notes.append("личная dev-подпись Apple ID")
    elif report.decrypted:
        report.type = "decrypted"
        report.type_label = "декрипт"
        if report.signed and report.signature_kind == "app_store":
            report.notes.append("расшифрован, осталась подпись App Store")
        elif report.signed:
            report.notes.append("расшифрован, чужая или магазинная подпись — нужна своя")
        if report.has_sinf:
            report.notes.append("SC_Info/*.sinf ещё лежит внутри — похоже на дамп")
    elif report.signed:
        report.type = "signed"
        report.type_label = "подписан"
    else:
        report.type = "unsigned"
        report.type_label = "без подписи и без FairPlay"
    report.ok = True
    return report


def iter_ipas(paths: list[Path]) -> list[Path]:
    out: list[Path] = []
    for path in paths:
        if path.is_dir():
            out.extend(sorted(path.rglob("*.ipa")))
        else:
            out.append(path)
    return out


def print_human(report: IpaReport) -> None:
    title = Path(report.path).name
    if not report.ok:
        print(f"{title}: ошибка — {report.error}")
        return
    print(f"{title}")
    print(f"  тип:        {report.type_label} ({report.type})")
    print(f"  FairPlay:   {'зашифрован' if report.encrypted else 'декрипт'}")
    print(f"  подпись:    {report.signature_kind or ('нет' if not report.signed else 'есть')}")
    if report.signer:
        print(f"  сертификат: {report.signer}")
    if report.team_id:
        print(f"  Team ID:    {report.team_id}")
    if report.name or report.bundle_id:
        print(f"  приложение: {report.name or '—'}  {report.bundle_id or ''}")
    if report.version:
        print(f"  версия:     {report.version}")
    if report.apple_id:
        print(f"  Apple ID:   {report.apple_id}")
    if report.adam_id:
        print(f"  adamId:     {report.adam_id}")
    print(f"  SC_Info:    {'да' if report.has_sinf else 'нет'}")
    if report.slices:
        bits = []
        for sl in report.slices:
            extra = f" cryptid={sl.get('cryptid')}"
            bits.append(f"{sl.get('cpu')}{extra}")
        print(f"  срезы:      {', '.join(bits)}")
    for note in report.notes:
        print(f"  • {note}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Тип IPA: декрипт / подписан / зашифрован под Apple ID",
    )
    parser.add_argument("paths", nargs="+", type=Path, help="файл .ipa или папка")
    parser.add_argument("--json", action="store_true", help="вывести JSON")
    args = parser.parse_args(argv)
    files = iter_ipas(args.paths)
    if not files:
        print("IPA не найдены", file=sys.stderr)
        return 2
    reports = [classify_ipa(path) for path in files]
    if args.json:
        print(json.dumps([asdict(r) for r in reports], ensure_ascii=False, indent=2))
    else:
        for i, report in enumerate(reports):
            if i:
                print()
            print_human(report)
    return 0 if all(r.ok for r in reports) else 1


if __name__ == "__main__":
    raise SystemExit(main())
