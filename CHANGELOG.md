# Changelog

## v0.2.19

- Kept `[services].auto_discover` as a backward-compatible config field while making full service inventory collection unconditional.
- Added a startup warning when `auto_discover=false` is set, explaining that alert rules and explicit service checks now control alerting instead of hiding collected services.

## v0.2.18

- Expanded VM and LXC service collection to ingest all services plus running and failed service views instead of relying on compact previews or hardcoded service lists.
- Added service classification and listening-port mapping for display so application services like `apache2`, `php8.3-fpm`, and `ssh` are prioritized ahead of noisy system units.
- Sent full service state, counts, classifications, descriptions, and ports over VM/LXC detail WebSocket events.
- Updated the dashboard guest inventory with running/total/failed counts, prioritized compact service chips, and a full service table with search and filters.

## v0.2.17

- Fixed QEMU Guest Agent exec by sending Proxmox's required JSON array payload, parsing `pid`, polling `exec-status`, and honoring `exitcode`, `out-data`, and `err-data`.
- Switched VM service discovery to plain `systemctl` output and preserved service load state, active state, sub-state, description, running, and failed flags.
- Kept `agent=true` when native guest-agent endpoints work even if guest exec fails; exec failures are logged as `guest-agent exec_error`.
- Added `[alerts].ignore_template_guests = true` by default so stopped templates no longer emit built-in critical `GuestDown` alerts.

## v0.2.16

- Fixed QEMU Guest Agent method handling by using `POST` for `/agent/ping` and `/agent/exec`, while keeping native read endpoints on `GET`.
- Added native guest-agent OS and filesystem collection through `get-osinfo` and `get-fsinfo` before falling back to guest exec.
- Improved `doctor` with guest-agent method and permission diagnostics for `VM.GuestAgent.Audit` and `VM.GuestAgent.Unrestricted`.
- Updated documentation for the guest-agent permissions required for full VM OS, IP, mount, service, and process visibility.

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
