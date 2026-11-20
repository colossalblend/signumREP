from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path

EXCLUDED_DIRS = {
    '.git', '.gradle', '.cache', '__pycache__', 'node_modules', 'target', 'build',
    'dist', 'frontend-dist', 'resources', 'apple-tools', 'linux_x64', 'linux_arm64',
    'third_party', 'vendor'
}

C_LIKE = {'.rs', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.c', '.cc', '.cpp', '.h', '.hh', '.hpp', '.kt', '.kts', '.java', '.cs', '.swift', '.php', '.gradle'}
BLOCK_ONLY = {'.css', '.scss'}
HASH_LIKE = {'.py', '.sh', '.toml', '.yaml', '.yml', '.properties'}
HTML_LIKE = {'.html', '.htm', '.xml', '.plist'}
SQL_LIKE = {'.sql'}
POWERSHELL = {'.ps1'}
VUE = {'.vue'}


@dataclass
class Result:
    changed: bool
    removed: int


def blank(value: str) -> str:
    return ''.join('\n' if ch == '\n' else '\r' if ch == '\r' else ' ' for ch in value)


def hash_boundary(source: str, index: int) -> bool:
    return index == 0 or source[index - 1].isspace()


def strip_lexical(
    source: str,
    line_markers: tuple[str, ...] = (),
    block_markers: tuple[tuple[str, str], ...] = (),
    hash_comments: bool = False,
    keep_shebang: bool = False,
    rust_lifetimes: bool = False,
    rust_raw_strings: bool = False,
) -> tuple[str, int]:
    output: list[str] = []
    removed = 0
    i = 0
    length = len(source)
    state = 'normal'
    quote = ''
    triple = ''

    while i < length:
        if state == 'normal':
            if rust_raw_strings:
                prefix_length = 0
                if source.startswith('br', i):
                    prefix_length = 2
                elif source.startswith('r', i):
                    prefix_length = 1
                if prefix_length:
                    marker = i + prefix_length
                    while marker < length and source[marker] == '#':
                        marker += 1
                    if marker < length and source[marker] == '"':
                        closing = '"' + ('#' * (marker - (i + prefix_length)))
                        end = source.find(closing, marker + 1)
                        end = length if end < 0 else end + len(closing)
                        output.append(source[i:end])
                        i = end
                        continue
            if source.startswith('"""', i) or source.startswith("'''", i):
                triple = source[i:i + 3]
                output.append(triple)
                i += 3
                state = 'triple'
                continue
            ch = source[i]
            if ch in {'"', "'", '`'}:
                if ch == "'" and rust_lifetimes:
                    next_ch = source[i + 1] if i + 1 < length else ''
                    next_next = source[i + 2] if i + 2 < length else ''
                    is_char_literal = next_ch == '\\' or next_next == "'"
                    if not is_char_literal:
                        output.append(ch)
                        i += 1
                        continue
                quote = ch
                output.append(ch)
                i += 1
                state = 'quote'
                continue
            if hash_comments and ch == '#' and hash_boundary(source, i):
                if keep_shebang and i == 0 and source.startswith('#!', i):
                    output.append(ch)
                    i += 1
                    continue
                end = source.find('\n', i)
                end = length if end < 0 else end
                output.append(blank(source[i:end]))
                removed += 1
                i = end
                continue
            matched_block = next(((start, end) for start, end in block_markers if source.startswith(start, i)), None)
            if matched_block:
                start, end_marker = matched_block
                end = source.find(end_marker, i + len(start))
                end = length if end < 0 else end + len(end_marker)
                output.append(blank(source[i:end]))
                removed += 1
                i = end
                continue
            matched_line = next((marker for marker in line_markers if source.startswith(marker, i)), None)
            if matched_line:
                end = source.find('\n', i)
                end = length if end < 0 else end
                output.append(blank(source[i:end]))
                removed += 1
                i = end
                continue
            output.append(ch)
            i += 1
            continue

        if state == 'quote':
            ch = source[i]
            output.append(ch)
            i += 1
            if ch == '\\' and i < length:
                output.append(source[i])
                i += 1
            elif ch == quote:
                state = 'normal'
            continue

        if state == 'triple':
            if source.startswith(triple, i):
                output.append(triple)
                i += 3
                state = 'normal'
            else:
                output.append(source[i])
                i += 1
            continue

    return ''.join(output), removed


def strip_html(source: str) -> tuple[str, int]:
    output: list[str] = []
    removed = 0
    i = 0
    while i < len(source):
        if source.startswith('<!--', i):
            end = source.find('-->', i + 4)
            end = len(source) if end < 0 else end + 3
            output.append(blank(source[i:end]))
            removed += 1
            i = end
        else:
            output.append(source[i])
            i += 1
    return ''.join(output), removed


def strip_powershell(source: str) -> tuple[str, int]:
    interim, block_removed = strip_lexical(source, block_markers=(('<#', '#>'),))
    cleaned, line_removed = strip_lexical(interim, hash_comments=True)
    return cleaned, block_removed + line_removed


def clean_file(path: Path) -> Result:
    suffix = path.suffix.lower()
    try:
        source = path.read_text(encoding='utf-8')
        encoding = 'utf-8'
    except UnicodeDecodeError:
        source = path.read_text(encoding='utf-8-sig')
        encoding = 'utf-8-sig'

    cleaned = source
    removed = 0
    if suffix in C_LIKE:
        markers = ('//', '#') if suffix == '.php' else ('//',)
        cleaned, removed = strip_lexical(source, line_markers=markers, block_markers=(('/*', '*/'),), rust_lifetimes=suffix == '.rs', rust_raw_strings=suffix == '.rs')
    elif suffix in BLOCK_ONLY:
        cleaned, removed = strip_lexical(source, block_markers=(('/*', '*/'),))
    elif suffix in HASH_LIKE:
        cleaned, removed = strip_lexical(source, hash_comments=True, keep_shebang=suffix == '.sh')
    elif suffix in HTML_LIKE:
        cleaned, removed = strip_html(source)
    elif suffix in SQL_LIKE:
        cleaned, removed = strip_lexical(source, line_markers=('--',), block_markers=(('/*', '*/'),))
    elif suffix in POWERSHELL:
        cleaned, removed = strip_powershell(source)
    elif suffix in VUE:
        cleaned, removed = strip_html(source)
        cleaned, c_removed = strip_lexical(cleaned, line_markers=('//',), block_markers=(('/*', '*/'),))
        removed += c_removed

    if cleaned != source:
        path.write_text(cleaned, encoding=encoding, newline='')
        return Result(True, removed)
    return Result(False, 0)


def should_process(path: Path) -> bool:
    return not any(part in EXCLUDED_DIRS for part in path.parts) and path.suffix.lower() in (C_LIKE | BLOCK_ONLY | HASH_LIKE | HTML_LIKE | SQL_LIKE | POWERSHELL | VUE)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('roots', nargs='+', type=Path)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()

    report: dict[str, object] = {'roots': [str(root) for root in args.roots], 'files_changed': 0, 'comments_removed': 0, 'files': []}
    file_records: list[dict[str, object]] = []
    for root in args.roots:
        for path in sorted(root.rglob('*')):
            if not path.is_file() or not should_process(path.relative_to(root)):
                continue
            result = clean_file(path)
            if result.changed:
                file_records.append({'path': str(path), 'comments_removed': result.removed})
                report['files_changed'] = int(report['files_changed']) + 1
                report['comments_removed'] = int(report['comments_removed']) + result.removed
    report['files'] = file_records
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f"files_changed={report['files_changed']}")
    print(f"comments_removed={report['comments_removed']}")
    print(f"report={args.report}")


if __name__ == '__main__':
    main()
