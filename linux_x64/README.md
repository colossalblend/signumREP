# Linux x86_64 helpers

`ipatool-v5` is the official Sorvigolova/ipatool 1.3.2 release and is selected by default. SAP signing and KBSync are embedded; it needs no separate signer. Release provenance and checksums are recorded in `SOURCE.json`.

`ipatool` here is the Go sapfix build. Apple rejects unsigned Store authenticate with HTTP 403.

Place a Linux-compatible `sapsigner` next to `ipatool` (or set `IPATOOL_SAP_HELPER`). The helper signs `X-Apple-ActionSignature` the same way Windows does. Without it, login may work but App Store restore/version checks cannot authenticate.

The full Signum `.deb` / AppImage must be built on a Linux machine (`scripts/build-linux.sh`).
