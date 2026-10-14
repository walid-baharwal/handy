# Handy

Handy is a local-first desktop app for starting, stopping, and watching all the services used by your development projects.

This first implementation includes:

- Projects with any shell command and a relative working directory
- Detection of `package.json` scripts and standard Docker Compose files
- Reusable nested groups with cycle validation and deduplicated commands
- Project, group, and individual Run/Stop controls
- Shared-service ownership: stopping one group keeps commands needed by another active group
- Per-command and merged live logs with smart scroll following and a 32 MiB session limit
- Optional status commands that detect services started or stopped outside Handy
- Versioned JSON persistence with an automatic backup; no database or cloud service
- Linux, macOS, and Windows process-tree termination

## Development

Requirements: Node.js 22+, pnpm 10+, Rust with `rustfmt`, and the [Tauri 2 system prerequisites](https://v2.tauri.app/start/prerequisites/).

On Ubuntu/Debian, install the native packages once:

```sh
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev pkg-config
```

Then run:

```sh
pnpm install
pnpm tauri dev
```

On Fedora / Red Hat style systems, install the native packages once:

```sh
sudo dnf install webkit2gtk4.1-devel gtk3-devel gcc gcc-c++ make curl wget file openssl-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel dbus-devel pkgconf-pkg-config rustfmt
```

On Arch, install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) with `pacman` before running the same development commands. If you installed Rust with `rustup`, run `rustup component add rustfmt` when `cargo fmt` is missing.

Checks:

```sh
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
pnpm test
```

The `--no-default-features` test runs the portable configuration and graph tests without linking the OS desktop libraries.
On Windows, `pnpm test` adds a Common Controls manifest to Cargo's test executables before running them. Tauri supplies the shipped app's manifest during its normal build.

## Linux installers

Handy can be packaged as native Linux installers and an AppImage with Tauri. Use the DEB on Debian or Ubuntu, the RPM on Fedora and compatible systems, and the AppImage on Arch or other distributions. The AppImage needs a compatible glibc and desktop libraries; it does not guarantee support for every Linux system.

Build the Linux packages and AppImage:

```sh
pnpm bundle
```

Build only a Debian package:

```sh
pnpm bundle:deb
```

Build only an RPM package:

```sh
pnpm bundle:rpm
```

The generated installers are written under `src-tauri/target/release/bundle/`, including:

- `deb/` for Debian and Ubuntu
- `rpm/` for Fedora and compatible distributions
- `appimage/` for an AppImage that works across many distributions

Install locally with:

```sh
sudo apt install ./src-tauri/target/release/bundle/deb/*.deb
sudo dnf install ./src-tauri/target/release/bundle/rpm/*.rpm
chmod +x src-tauri/target/release/bundle/appimage/*.AppImage
./src-tauri/target/release/bundle/appimage/*.AppImage
```

Run the command for your distribution. `apt` and `dnf` install the package's runtime dependencies. An AppImage can require FUSE support on your system; see the [AppImage troubleshooting guide](https://docs.appimage.org/user-guide/troubleshooting/fuse.html).

For widest Linux compatibility, build on an older supported base such as Ubuntu 22.04 or Debian 12 so the resulting binaries do not require a newer glibc than your target systems provide. This follows Tauri's Linux distribution guidance.

## GitHub releases

A `v<version>` tag builds Linux x64 DEB, RPM, and AppImage files, a Windows x64 setup EXE, and DMGs for Intel and Apple Silicon Macs. The release workflow checks package contents, installs the DEB on Ubuntu and the RPM on Fedora, extracts the AppImage on Arch, and creates a draft GitHub release with checksums. Check the installers on real desktops before publishing the draft.

The Windows installer may show a Microsoft SmartScreen warning because it has no code signing certificate. The macOS DMGs use ad hoc signing, so macOS may ask you to approve Handy in Privacy & Security. Follow the [Apple instructions for opening an unverified app](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac). A trusted public release needs an Apple Developer certificate and notarization, and a Windows signing certificate.

For a desktop smoke test on each platform, install Handy, add a project with a long-running command, run it, check stdout and stderr, stop it, quit from the tray, and reopen Handy to check that the project remains saved. On Linux, repeat this with the AppImage on Arch or another non-DEB/non-RPM distribution. If installation fails, capture the package manager output; for an RPM, `rpm -qpR Handy*.rpm` lists its dependencies.

## Storage and trust

Configuration is stored as `config.v1.json` under Tauri's application data directory. Commands are executed through the user's login shell and have the same permissions as Handy, so only add or import commands you trust.

## External service status

Set a command's optional status command when you want Handy to recognize a service started from another terminal or before Handy opened. Handy runs the check from the command's working directory when it starts, then every 15 seconds.

- Exit code `0` means the service is running.
- Any other exit code means the service is stopped.
- Handy stops a service it detected outside the app only when you configure a stop command.
- Quitting Handy leaves externally started services running.

Docker Compose suggestions include status and stop commands. Handy treats the Compose command as running while at least one container reports a running state.

Handy keeps direct child-process tracking for commands without a status command. Process-name matching is not used because unrelated programs can share the same executable or command line.

## Next slices

Selective `.handy.json` import/export, tray/quit behavior, single-instance handling, signed installers, and GitHub-based updates remain on the v1 roadmap.

## License

MIT
