#!/usr/bin/env bash
# DEVELOPER SCRIPT — build-time only. It is NOT shipped inside Signum, is not
# run by the installer, and never touches a user's Mac. You run it once on your
# own machine; what ships is the binaries it leaves in the repo.
#
# It populates apple-tools/macos-arm64/support/ with native Apple Silicon device
# tools, wired the same way as the Intel set: every non-system dylib sits next
# to the binaries and is referenced as @executable_path/<name>, so the shipped
# .app needs nothing installed on the user's Mac — no Homebrew, no downloads.
#
# Run on an Apple Silicon Mac with Homebrew, then commit the result:
#   bash scripts/dev-prepare-mac-arm64-tools.sh
#
# Without this the universal build falls back to the Intel tools through
# Rosetta 2 (it works, but Rosetta stays a requirement).
set -euo pipefail
cd "$(dirname "$0")/.."

DEST="apple-tools/macos-arm64/support"
TOOLS=(ideviceinfo ideviceinstaller idevicepair)

die() { echo "error: $*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "macOS only"
[[ "$(uname -m)" == "arm64" ]] || die "run this on Apple Silicon, not under Rosetta (uname -m says $(uname -m))"
BREW="$(command -v brew || true)"
[[ -n "${BREW}" ]] || die "Homebrew not found; install it from https://brew.sh first"

echo "==> brew install libimobiledevice ideviceinstaller"
"${BREW}" install --quiet libimobiledevice ideviceinstaller
PREFIX="$("${BREW}" --prefix)"

mkdir -p "${DEST}"
rm -f "${DEST}"/* 2>/dev/null || true

for tool in "${TOOLS[@]}"; do
  src="${PREFIX}/bin/${tool}"
  [[ -x "${src}" ]] || die "${src} missing after brew install"
  cp -f "${src}" "${DEST}/${tool}"
  chmod u+w "${DEST}/${tool}"
done

# Dependencies of a Mach-O file, minus the OS-provided ones and its own id.
deps_of() {
  local file="$1" self
  self="$(basename "${file}")"
  otool -L "${file}" | tail -n +2 | awk '{print $1}' | while read -r dep; do
    case "${dep}" in
      /usr/lib/*|/System/*) continue ;;
    esac
    [[ "$(basename "${dep}")" == "${self}" ]] && continue
    echo "${dep}"
  done
}

# Copy every dependency into DEST, following the chain.
collect() {
  local file="$1" dep base real
  while read -r dep; do
    [[ -n "${dep}" ]] || continue
    base="$(basename "${dep}")"
    [[ -f "${DEST}/${base}" ]] && continue
    real="${dep}"
    case "${dep}" in
      @rpath/*|@loader_path/*|@executable_path/*) real="${PREFIX}/lib/${base}" ;;
    esac
    [[ -f "${real}" ]] || die "cannot resolve ${dep} (looked at ${real})"
    cp -f "${real}" "${DEST}/${base}"
    chmod u+w "${DEST}/${base}"
    collect "${DEST}/${base}"
  done < <(deps_of "${file}")
}

echo "==> collecting dylibs"
for tool in "${TOOLS[@]}"; do
  collect "${DEST}/${tool}"
done

echo "==> rewriting install names to @executable_path"
for file in "${DEST}"/*; do
  base="$(basename "${file}")"
  chmod u+w "${file}"
  case "${base}" in
    *.dylib) install_name_tool -id "@executable_path/${base}" "${file}" ;;
  esac
  while read -r dep; do
    [[ -n "${dep}" ]] || continue
    dbase="$(basename "${dep}")"
    [[ "${dep}" == "@executable_path/${dbase}" ]] && continue
    [[ -f "${DEST}/${dbase}" ]] || die "${base} needs ${dbase}, which was not collected"
    install_name_tool -change "${dep}" "@executable_path/${dbase}" "${file}"
  done < <(deps_of "${file}")
  # Rewriting a load command invalidates the signature, and macOS refuses to
  # run an arm64 binary with a broken one. Ad-hoc signing is what the Intel set
  # carries too.
  codesign --force --sign - "${file}" >/dev/null 2>&1 || die "codesign failed for ${base}"
  chmod +x "${file}"
done

echo "==> verifying"
fail=0
for file in "${DEST}"/*; do
  base="$(basename "${file}")"
  if ! lipo -archs "${file}" 2>/dev/null | grep -q arm64; then
    echo "  BAD  ${base}: not arm64 ($(lipo -archs "${file}" 2>/dev/null))"; fail=1; continue
  fi
  leaked="$(deps_of "${file}" | grep -v '^@executable_path/' || true)"
  if [[ -n "${leaked}" ]]; then
    echo "  BAD  ${base}: still points outside the bundle:"; echo "${leaked}" | sed 's/^/       /'; fail=1; continue
  fi
  codesign -v "${file}" 2>/dev/null || { echo "  BAD  ${base}: signature invalid"; fail=1; continue; }
  echo "  ok   ${base}"
done
[[ "${fail}" -eq 0 ]] || die "verification failed; nothing was written to SOURCE.json"

python3 - "$PREFIX" <<'PY'
import json, subprocess, sys, datetime, pathlib
prefix = sys.argv[1]
def brew_version(name):
    try:
        out = subprocess.check_output(["brew", "list", "--versions", name], text=True).split()
        return out[-1] if len(out) > 1 else "unknown"
    except Exception:
        return "unknown"
path = pathlib.Path("apple-tools/macos-arm64/SOURCE.json")
data = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
data["support"] = (
    f"Homebrew libimobiledevice {brew_version('libimobiledevice')} + "
    f"ideviceinstaller {brew_version('ideviceinstaller')} ({prefix}), "
    "dylibs copied next to the binaries and rebound to @executable_path, ad-hoc signed"
)
data["support_built_at"] = datetime.date.today().isoformat()
path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print("==> SOURCE.json updated")
PY

echo "==> done: $(ls -1 "${DEST}" | wc -l | tr -d ' ') files in ${DEST}"
