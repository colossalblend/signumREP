#!/usr/bin/env bash
                                                                                        
set -euo pipefail
export PATH="/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:${HOME}/.cargo/bin:${PATH}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(python3 -c "import json; print(json.load(open('${ROOT}/package.json'))['version'])" 2>/dev/null \
  || /usr/bin/plutil -extract CFBundleShortVersionString raw "${ROOT}/src-tauri/target/release/bundle/macos/Signum.app/Contents/Info.plist" 2>/dev/null \
  || echo "1.1.0")"
ARCH="$(uname -m)"
case "${ARCH}" in
  x86_64) ARCH_TAG="x64" ;;
  arm64)  ARCH_TAG="arm64" ;;
  *)      ARCH_TAG="${ARCH}" ;;
esac

# Prefer the universal bundle when it exists: that is the one that does not make
# macOS warn about Intel apps, and it is what release builds produce.
UNIVERSAL_APP="${ROOT}/src-tauri/target/universal-apple-darwin/release/bundle/macos/Signum.app"
APP_SRC="${ROOT}/src-tauri/target/release/bundle/macos/Signum.app"
HOST_ARCHS="${ARCH}"
if [[ -d "${UNIVERSAL_APP}" ]]; then
  APP_SRC="${UNIVERSAL_APP}"
  ARCH_TAG="universal"
  # A universal package must accept both machines. Left at the build host's own
  # arch, an Intel-built pkg simply refuses to install on Apple Silicon.
  HOST_ARCHS="x86_64,arm64"
fi
OUT_DIR="${ROOT}/src-tauri/target/release/bundle/installer"
DIST_DIR="${ROOT}/dist/macos"
WEB_DIR="${ROOT}/website/downloads/macos"
STAGE="${TMPDIR:-/tmp}/signum-pkg-stage-$$"
SCRIPTS_DIR="${TMPDIR:-/tmp}/signum-pkg-scripts-$$"
COMPONENT_PKG="${OUT_DIR}/Signum-component.pkg"
PKG_OUT="${OUT_DIR}/Signum_${VERSION}_${ARCH_TAG}.pkg"
DMG_OUT="${OUT_DIR}/Signum_${VERSION}_${ARCH_TAG}.dmg"
DIST_XML="${OUT_DIR}/distribution.xml"

echo "==> Signum macOS installer v${VERSION} (${ARCH_TAG})"

if [[ ! -d "${APP_SRC}" ]]; then
  echo "Signum.app not found at ${APP_SRC}" >&2
  echo "Build first: npm run build:mac   or   npx tauri build --bundles app" >&2
  exit 1
fi

                                                    
if [[ -f "${ROOT}/src-tauri/icons/icon.icns" ]]; then
  cp -f "${ROOT}/src-tauri/icons/icon.icns" "${APP_SRC}/Contents/Resources/icon.icns"
fi
xattr -cr "${APP_SRC}" 2>/dev/null || true
codesign --force --deep --sign - "${APP_SRC}" 2>/dev/null || true

mkdir -p "${OUT_DIR}" "${DIST_DIR}" "${WEB_DIR}"
rm -rf "${STAGE}" "${SCRIPTS_DIR}"
mkdir -p "${STAGE}" "${SCRIPTS_DIR}"

                                                                 
cat > "${SCRIPTS_DIR}/postinstall" <<'POST'
           
set -e
APP="/Applications/Signum.app"
if [[ -d "$APP" ]]; then
  xattr -cr "$APP" 2>/dev/null || true
                                                          
  codesign --force --deep --sign - "$APP" 2>/dev/null || true
fi
exit 0
POST
chmod 755 "${SCRIPTS_DIR}/postinstall"

echo "==> Building component package…"
                                                              
ditto "${APP_SRC}" "${STAGE}/Signum.app"

pkgbuild \
  --root "${STAGE}" \
  --scripts "${SCRIPTS_DIR}" \
  --identifier "pw.appbank.signum" \
  --version "${VERSION}" \
  --install-location "/Applications" \
  --ownership recommended \
  "${COMPONENT_PKG}"

echo "==> Building product archive (Installer.app)…"
cat > "${DIST_XML}" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
  <title>Signum ${VERSION}</title>
  <organization>pw.appbank</organization>
  <domains enable_localSystem="true" enable_currentUserHome="true"/>
  <options customize="never" require-scripts="false" hostArchitectures="${HOST_ARCHS}"/>
  <welcome file="welcome.html" mime-type="text/html"/>
  <conclusion file="conclusion.html" mime-type="text/html"/>
  <pkg-ref id="pw.appbank.signum"/>
  <choices-outline>
    <line choice="default">
      <line choice="pw.appbank.signum"/>
    </line>
  </choices-outline>
  <choice id="default"/>
  <choice id="pw.appbank.signum" visible="false">
    <pkg-ref id="pw.appbank.signum"/>
  </choice>
  <pkg-ref id="pw.appbank.signum" version="${VERSION}" onConclusion="none">Signum-component.pkg</pkg-ref>
</installer-gui-script>
EOF

                                      
RES="${OUT_DIR}/pkg-resources"
mkdir -p "${RES}"
cat > "${RES}/welcome.html" <<EOF
<!DOCTYPE html>
<html lang="ru">
<head><meta charset="utf-8"><style>
body{font-family:-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;color:#222;margin:16px 20px;line-height:1.45}
h1{font-size:18px;margin:0 0 10px}
</style></head>
<body>
<h1>Установка Signum ${VERSION}</h1>
<p>Signum — установка и подпись iOS‑приложений с компьютера.</p>
<ul>
  <li>Приложение будет установлено в <b>Программы</b> (/Applications)</li>
  <li>macOS ${ARCH_TAG} · версия ${VERSION}</li>
  <li>После установки откройте Signum из Launchpad или папки «Программы»</li>
</ul>
<p>Нажмите «Продолжить», чтобы начать установку.</p>
</body></html>
EOF

cat > "${RES}/conclusion.html" <<EOF
<!DOCTYPE html>
<html lang="ru">
<head><meta charset="utf-8"><style>
body{font-family:-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;color:#222;margin:16px 20px;line-height:1.45}
h1{font-size:18px;margin:0 0 10px}
</style></head>
<body>
<h1>Signum установлен</h1>
<p>Откройте <b>Signum</b> из папки «Программы».</p>
<p>Если macOS напишет, что разработчик не установлен: ПКМ по приложению → <b>Открыть</b> → <b>Открыть</b>.</p>
<p>Подключите iPhone кабелем, разблокируйте его и нажмите «Доверять».</p>
</body></html>
EOF

productbuild \
  --distribution "${DIST_XML}" \
  --resources "${RES}" \
  --package-path "${OUT_DIR}" \
  "${PKG_OUT}"

echo "==> Building DMG…"
DMG_STAGE="${TMPDIR:-/tmp}/signum-dmg-$$"
rm -rf "${DMG_STAGE}"
mkdir -p "${DMG_STAGE}"
ditto "${APP_SRC}" "${DMG_STAGE}/Signum.app"
ln -sf /Applications "${DMG_STAGE}/Applications"
rm -f "${DMG_OUT}"
hdiutil create \
  -volname "Signum ${VERSION}" \
  -srcfolder "${DMG_STAGE}" \
  -ov \
  -format UDZO \
  -imagekey zlib-level=9 \
  "${DMG_OUT}"
rm -rf "${DMG_STAGE}"

                                               
mkdir -p "${ROOT}/src-tauri/target/release/bundle/dmg"
cp -f "${DMG_OUT}" "${ROOT}/src-tauri/target/release/bundle/dmg/"

echo "==> Copying to dist + website…"
cp -f "${PKG_OUT}" "${DIST_DIR}/"
cp -f "${DMG_OUT}" "${DIST_DIR}/"
cp -f "${PKG_OUT}" "${WEB_DIR}/"
cp -f "${DMG_OUT}" "${WEB_DIR}/"

                                                        
rm -f "${COMPONENT_PKG}"
rm -rf "${STAGE}" "${SCRIPTS_DIR}"

echo ""
echo "=== Installers ready ==="
ls -lah "${PKG_OUT}" "${DMG_OUT}"
echo ""
echo "PKG (Installer.app):  ${PKG_OUT}"
echo "DMG (drag & drop):    ${DMG_OUT}"
echo "Website folder:       ${WEB_DIR}/"
echo "Dist folder:          ${DIST_DIR}/"
echo "OK"
