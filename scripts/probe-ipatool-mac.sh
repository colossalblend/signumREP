#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

IP="/Applications/Signum.app/Contents/Resources/resources/apple-tools/macos-x64/v2/ipatool"
if [[ ! -x "$IP" ]]; then
  IP="/Users/user/Signum-v1/apple-tools/macos-x64/v2/ipatool"
fi
echo "IPATOOL=$IP"
file "$IP"
xattr -cr "$IP" 2>/dev/null || true
"$IP" --help 2>&1 | head -50 || true
echo "==== auth help ===="
"$IP" auth --help 2>&1 | head -60 || true
echo "==== auth login help ===="
"$IP" auth login --help 2>&1 | head -80 || true
echo "==== version ===="
"$IP" --version 2>&1 || true
echo "==== which anisette deps ===="
                                                 
ls -la "$(dirname "$IP")" 2>/dev/null || true
                                                                        
echo "==== probe login shape (expect fail) ===="
set +e
OUT=$("$IP" auth login -e "probe@example.com" -p "not-a-real-password" 2>&1)
EC=$?
set -e
echo "exit=$EC"
echo "$OUT" | head -40
