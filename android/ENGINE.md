# Android Store engine

Signum Android uses its Kotlin App Store client (`AppStoreClient`) and the bundled
Android SAP helper (`libsapsigner.so`). It does not run the desktop ipatool executable.

## 2026-10-15: routing fixes adapted from ipatool 1.1.8

Upstream: https://github.com/Sorvigolova/ipatool/compare/1.1.4...1.1.8

- A DNS failure for a pod-specific Store host retries once through
  `buy.itunes.apple.com`, preserving the request path and query.
- Redirects retain Apple's complete Location, including cookie-handshake parameters.
- The `pod` response header, `itspod` cookie, and redirected Store hostname update
  the active route. Updated routing and cookies are saved with the active account.
- Authentication can try its remaining supported endpoints after a DNS failure.
- Redirect loops are bounded. DNS failures on generic hosts and non-DNS transport
  failures are not mistaken for a stale pod or a missing app purchase.
- Protected requests rebuild their headers after redirects so new cookies are used.

These fixes are applied to Android's authenticated Store requests and login using
OkHttp. Android purchase history continues to use its existing Report a Problem client.
The native SAP helper is unchanged. Upstream's Windows console fixes, C++ binary
stripping, libcurl migration, and OpenSSL-to-mbedTLS migration do not replace
Android's existing networking or cryptography libraries.

Validation: six isolated JVM routing tests passed without building an APK.
Live Apple ID login/download and SAP behavior on an Android device are not verified
by those tests.

## 2026-11-22: parity with ipatool 1.3.2

Upstream: https://github.com/Sorvigolova/ipatool/releases/tag/1.3.2 (ipatool-cpp,
the same v5 engine the desktop app bundles). Pure protocol logic lives in
`data/apple/StoreProtocol.kt`, covered by the JVM tests in `StoreProtocolTest`.
`AppStoreClient` handles the network side.

| ipatool command | Android |
| --- | --- |
| `auth login` (incl. 2FA) | `login()` / `authenticate()`: SAP-signed body. 2FA means an empty failureType plus `MZFinance.BadLogin.Configurator_message`. `-5000` retries attempt 1. Also handles a disabled account. 5xx gets up to 3 sends (250 ms × try) and 429 stops. DSID comes from the `x-dsid` header. The native endpoint uses `attempt=2` with a code. Storefront and pod come from the response, the name from `accountInfo.address` (persisted). |
| `auth info` | `accountInfo()`: email, name, DSID, storefront plus country code (ipatool storefront table) |
| `auth revoke` | `revoke()` drops the Store session. Full logout stays `SignumEngine.logout()`. |
| `search` | `searchAccount(term, limit)`: iTunes Search in the account's storefront country, `entity=software,iPadSoftware`, ipatool URL encoding. The catalog `search()` is kept. |
| `purchase` | `buy()`: `p<pod>-buy…/MZFinance.woa/wa/buyProduct` with the full ipatool payload. Checks run in ipatool order: 2059 retries with `GAME`, 5002 means already owned, then Subscription Required, then 2034/"Sign In" triggers a silent re-login, then `purchaseSuccess`/status 0. Paid apps are refused. Accepting the terms dialog is a Signum addition. |
| `download` | `resolve_download` cascade: volumeStoreDownloadProduct (2042 authorization dialog retries with `hasBeenAuthedForBuy`), then redownloadProduct pinned to the version from MZStorePlatform `mdm-lockup` (enterprisestore, iphone, ipad), then updateProduct with strict validation (itemId, version, bundle ID). A response counts as served when it has an item with sinf. "Redownload Unavailable…" (no-break-space aware) and 9610 raise LicenseRequired, which triggers a purchase and one retry (`--purchase`). 2034/2042/1008/"Sign In" allow one silent re-login. Also: resumable CDN download, md5 check with one re-download, songDownloadDone ping. Sinf placement uses `SC_Info/Manifest.plist` or `<exec>.sinf`. iTunesMetadata.plist is patched like iTunes (appleId, userName, downloadInfo, purchaseDate, is-purchased-redownload, storeCohort). |
| `list-versions` | `listVersionIds()` / `listVersions()` use the same cascade without requiring sinf. Not owned means an empty list. |
| `get-version-metadata` | `versionMetadata()`: bundleShortVersionString, releaseDate, plus the minimum iOS from Info.plist via range requests (`.ipa` only), cached |
| bag.xml | `bag()`: `?guid=&ix=6`, Configurator 2.21 UA. Handles the signed `{bag=<data>}`, `urlBag` and flat forms. Falls back to defaults on failure. |

Intentional differences from desktop:

- kbsync and X-Apple-AMD (storeagent/CoreADI emulation) are not available on
  Android. volumeStoreDownloadProduct keeps its passwordToken form, and login uses
  MZFinance authenticate with SAP and anisette instead of native + ADI.
- No macOS `.pkg` download or dpInfo decryption (iOS only).
- iTunesArtwork is not added (packaging belongs to `IpaTweaks`).
- `list-purchases` remains `listPurchasesViaLibrary()` from the 1.1.8 port.

Validation: the `StoreProtocolTest` (17) and `StorePodRoutingTest` (6) tests pass
on a plain JVM. `AppStoreClient.kt` compiles against stubbed Android classes.
Not verified: the APK build with the Android SDK (CI does this) and live
login/purchase/download on a device.
