# Changelog

## v0.2.15

- Added a dedicated tag-driven release workflow that publishes the Linux binary, Debian package, checksum file, systemd unit, and release config example.
- Hardened `install.sh` for real GitHub release assets, checksum verification, `.deb` install, binary fallback, and config preservation on upgrades.
- Added parser fixture coverage and real-world testing documentation for release readiness across common Proxmox environments.
- Kept README release artifact and alert-channel claims aligned with what the project actually ships.

## v0.2.14

- Fixed release-readiness refactor compile issues in `init` and runtime cluster wiring.
- Built the frontend before Rust clippy/tests in CI so `rust-embed` always has dashboard assets.
- Supersedes v0.2.13, whose GitHub Actions run exposed the refactor issues above.

## v0.2.13

- Re-ran repository-wide Rust formatting so the public CI release checks pass cleanly.
- Supersedes v0.2.12, whose GitHub Actions run stopped at `cargo fmt --check`.

## v0.2.12

- Added shared custom alert rule evaluator state across node, guest, storage, LXC, and VM service checks.
- Improved service rule state handling for running, failed, inactive, dead, activating, unknown, and missing services.
- Confirmed VM service checks support VMID matching with IP fallback, and custom VM service rules now evaluate from VM service state when guest visibility is available.
- Added an alert channel abstraction so new providers can be added without growing the dispatcher.
- Split CLI/init/doctor/runtime code out of `main.rs`.
- Added CI workflow for fmt, clippy, tests, and frontend build.
- Added public release docs and project governance files.
- Expanded README documentation for custom alert rules and backup policy.

## v0.2.11

- Kept `/health` public while dashboard, API, WebSocket, and metrics are protected when auth is configured.

## v0.2.10

- Split platform collectors into focused modules.
- Added backup policy, tag rules, exclusions, real backup artifact detection, custom alert rules, parser tests, and release packaging improvements.
