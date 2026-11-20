#!/usr/bin/env bash
set -uo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$HOME/.cargo/bin"

IP="/Applications/Signum.app/Contents/Resources/resources/macos-x64/ipatool"
echo "IP=$IP"
file "$IP"

                                      
PASS=""
if [[ -f "$HOME/.signum/keychain_passphrase" ]]; then
  PASS=$(cat "$HOME/.signum/keychain_passphrase" | tr -d '\n')
  echo "pass_from_home_signum len=${#PASS}"
fi
if [[ -f "$HOME/Library/Application Support/pw.appbank.signum/keychain_passphrase" ]]; then
  PASS2=$(cat "$HOME/Library/Application Support/pw.appbank.signum/keychain_passphrase" | tr -d '\n')
  echo "pass_from_appsupport len=${#PASS2}"
  if [[ -n "$PASS2" ]]; then PASS="$PASS2"; fi
fi
                                       

echo "==== auth info ===="
set +e
"$IP" --keychain-passphrase "${PASS:-test}" auth info --format json 2>&1 | head -30
echo "exit=$?"
set -e

echo "==== list-versions T-Bank (example id if known) ===="
                                                                                           
ADAM=""
if [[ -f "$HOME/Library/Application Support/pw.appbank.signum/cache.json" ]]; then
  echo "cache exists"
                                                               
  python3 - <<'PY' 2>/dev/null || true
import json,re,os
p=os.path.expanduser("~/Library/Application Support/pw.appbank.signum/cache.json")
try:
  d=json.load(open(p))
  print(json.dumps(d, ensure_ascii=False)[:500])
except Exception as e:
  print("cache read", e)
PY
fi

                                                                                                             
                                                                             
echo "==== list-versions help shape ===="
"$IP" list-versions 2>&1 | head -20 || true

                                                                                           
                                                
for ADAM in 310633997 284882215 389801252; do
  echo "==== list-versions -i $ADAM (keychain+format before) ===="
  set +e
  OUT=$("$IP" --keychain-passphrase "${PASS:-x}" --format json list-versions -i "$ADAM" 2>&1)
  EC=$?
  set -e
  echo "exit=$EC"
  echo "$OUT" | head -40
  echo "==== list-versions -i $ADAM (no format first) ===="
  set +e
  OUT2=$("$IP" --keychain-passphrase "${PASS:-x}" list-versions -i "$ADAM" --format json 2>&1)
  EC2=$?
  set -e
  echo "exit=$EC2"
  echo "$OUT2" | head -40
                                         
  if echo "$OUT$OUT2" | grep -qiE 'externalVersion|version|success|error|login|purchas'; then
    break
  fi
done

echo "==== ~/.ipatool ===="
ls -la ~/.ipatool 2>/dev/null || echo none
