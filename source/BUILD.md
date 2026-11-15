# Build

The `source` directory contains the application source corresponding to the portable release. Original code is AGPL-3.0-only. Third-party components retain their own licenses.

Use Windows x64 with the Rust stable MSVC toolchain, Visual Studio C++ build tools, and the Rust documentation component used to collect standard-library license notices. The published build uses Rust 1.100.0 and wasm-bindgen-cli 0.2.130. Node is optional and only needed for the `npm run build` alias.

From this directory in PowerShell

```powershell
rustup target add wasm32-unknown-unknown
rustup component add rust-docs
cargo install wasm-bindgen-cli --version 0.2.130 --locked
New-Item -ItemType Directory -Path public,data -Force
Copy-Item '../Intelligence Index/public/*' public -Recurse -Force
Copy-Item '../Intelligence Index/data/public-models.raw.json.gz' data
cargo run --locked --release -p xtask -- build
```

The equivalent build command with npm is `npm run build`.

The seed files come from the matching portable release. When using the source ZIP, extract the matching portable ZIP beside the source folder first. Data and provider branding retain their separate terms described in `NOTICE`.

Cargo.lock pins dependencies fetched from crates.io. The build installs the app in the current user's Documents folder, preserves existing selections, and activates the installed server. It includes no test or browser automation steps.

`rust/native` contains the launcher, server, refresh, and enrichment. `rust/ui` contains the Rust WebAssembly interface. `rust/core` contains shared calculations and policies. `rust/xtask` contains the build and packaging workflow. `assets` contains the styles, fonts, icons, and their notices.
