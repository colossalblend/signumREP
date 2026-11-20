#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

IP="/Applications/Signum.app/Contents/Resources/resources/macos-x64/ipatool"
echo "==== with --non-interactive (as Signum does) ===="
set +e
OUT1=$("$IP" --non-interactive --keychain-passphrase testpass auth login -e "probe@example.com" -p "badpass" 2>&1)
EC1=$?
set -e
echo "exit=$EC1"
echo "$OUT1" | head -30

echo "==== without --non-interactive ===="
set +e
OUT2=$("$IP" --keychain-passphrase testpass auth login -e "probe@example.com" -p "badpass" 2>&1)
EC2=$?
set -e
echo "exit=$EC2"
echo "$OUT2" | head -30

echo "==== with -a fake code ===="
set +e
OUT3=$("$IP" --keychain-passphrase testpass auth login -e "probe@example.com" -p "badpass" -a "123456" 2>&1)
EC3=$?
set -e
echo "exit=$EC3"
echo "$OUT3" | head -30

echo "==== with --non-interactive and -a ===="
set +e
OUT4=$("$IP" --non-interactive --keychain-passphrase testpass auth login -e "probe@example.com" -p "badpass" -a "123456" 2>&1)
EC4=$?
set -e
echo "exit=$EC4"
echo "$OUT4" | head -30

echo "==== arg order: keychain after command? ===="
set +e
OUT5=$("$IP" auth login -e "probe@example.com" -p "badpass" --keychain-passphrase testpass 2>&1)
EC5=$?
set -e
echo "exit=$EC5"
echo "$OUT5" | head -30
