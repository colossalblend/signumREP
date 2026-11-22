# Signum Store metadata helper

C++ sources originate from [Sorvigolova/ipatool 1.1.1](https://github.com/Sorvigolova/ipatool/releases/tag/1.1.1),
commit `7fae0e2a241fab254bcce23ab02d315e914927a2`.
Authentication, downloads and metadata use this version in production.
The executable retains the name `signum-store` for wrapper compatibility.
Authentication and device identity code are upstream without changes.
There is no `IPATOOL_MAC` override: ipatool chooses the network adapter itself.
Build configuration embeds the same Apple assets and links the installed Linux
dependencies. The hosting wrapper supplies the bundled loader and CA trust
compatibility; TLS certificate validation remains enabled.

Metadata first uses the bag's entitlement download endpoint with the account's
cached kbsync. The fallback sends signed Store requests with passwordToken,
DSID and Configurator user agent, followed by DownloadDispatch updateProduct.
An empty reply does not prove a country restriction.

## Linux x86-64 build

Install CMake, a C++20 compiler, libcurl/OpenSSL development files, fmt,
nlohmann-json, minizip and Unicorn 2.1.4 development files. The Python wheel
`unicorn==2.1.4` also supplies its C headers and shared library.

```sh
python3 native_store/prepare_assets.py /opt/signum-bot/bin/ipatool
cmake -S native_store -B native_store/build \
  -DUNICORN_INCLUDE_DIR=/path/to/unicorn/include \
  -DUNICORN_LIBRARY=/path/to/unicorn/lib/libunicorn.so.2
cmake --build native_store/build -j1
cmake --install native_store/build --prefix /opt/signum-store-package
```

Ship `bin/signum-store` with `libunicorn.so.2` beside it and Unicorn's license.
The installed executable uses `$ORIGIN` to locate this library. The asset
manifest verifies all embedded assets before extracting them; another binary
layout fails closed. Assets and build outputs stay outside version control.

Never enable `--debug` on user-facing requests: upstream debug output contains
Apple credentials and response bodies. Keep account HOME isolated, preserve
the existing device identity and serialize commands that write its session.
