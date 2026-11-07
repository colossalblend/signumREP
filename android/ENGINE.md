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
