#!/usr/bin/env bash
set -uo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

IP="/Applications/Signum.app/Contents/Resources/resources/apple-tools/macos-x64/v2/ipatool"
IP2="/Users/user/Signum-v1/apple-tools/macos-x64/v2/ipatool"
IP3="/Users/user/Signum-v1/apple-tools/macos-x64/v3-legacy/ipatool"

echo "==== env ===="
uname -a
sw_vers
echo "HOME=$HOME"
echo "==== binaries ===="
file "$IP" "$IP2" "$IP3" 2>/dev/null
ls -la "$IP" "$IP2" "$IP3" 2>/dev/null
echo "==== data dirs ===="
ls -la ~/.ipatool 2>/dev/null || echo "no ~/.ipatool"
ls -la ~/.signum 2>/dev/null || echo "no ~/.signum"
ls -la "$HOME/Library/Application Support/pw.appbank.signum" 2>/dev/null || true
ls -la "$HOME/Signum" 2>/dev/null || true
             
rm -f ~/.ipatool/cookies.lock 2>/dev/null || true

echo "==== help (installed) ===="
"$IP" 2>&1 | head -40

echo "==== test fake login v2 installed ===="
set +e
OUT=$("$IP" --keychain-passphrase testdiag123 auth login -e "notreal@example.com" -p "WrongPass123!" 2>&1)
EC=$?
set -e
echo "exit=$EC"
echo "$OUT" | head -20

echo "==== test fake login v3 ===="
set +e
OUT3=$("$IP3" --keychain-passphrase testdiag123 auth login -e "notreal@example.com" -p "WrongPass123!" 2>&1)
EC3=$?
set -e
echo "exit=$EC3"
echo "$OUT3" | head -20

echo "==== check if app is running ===="
pgrep -l signum || echo "signum not running"
pgrep -l ipatool || echo "ipatool not running"

echo "==== recent console (signum) ===="
log show --predicate 'process == "signum" OR processImagePath CONTAINS "Signum"' --last 5m 2>/dev/null | tail -40 || true
