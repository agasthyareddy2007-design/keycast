# Keycast

## Update 0.1.1 — pairing connection fix

Replace the executable on **all PCs**, close old Keycast windows, and restart.
Copy the freshly generated main-PC pairing code to each receiver again.

Fixed a Windows socket-mode bug: incoming connections inherited nonblocking mode from the listener, but the pairing code expected blocking reads. A delayed authentication reply could therefore close an otherwise valid connection and show "failed to fill whole buffer" on the sender. Pairing now explicitly restores blocking mode with a timeout. This matches the inheritance behavior documented by [Microsoft](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept).

Connection errors now appear in the main status area with the pairing stage and recovery instructions. A different pairing code, paused receiver, or receiver already in use can also cause the connection to close; this update does not bypass pairing or the pause switch.

The four TCP regression tests now use a nonblocking listener and delay the authentication reply to reproduce the original timing condition. They still require a normal Windows environment with socket access.

A portable Rust application for mirroring physical keyboard input from one Windows PC to several selected PCs on your local Wi-Fi. Typing continues normally on the main PC. Keycast forwards key-down, key-up, auto-repeat, and modifier events to the focused window on each receiver.

## Installation & Running on Other PCs

You can install Keycast using either the **Setup Installer** (recommended) or the **Portable ZIP**:

### Option A: Using the Installer (Recommended)
1. Copy `Keycast-Setup.exe` (or `Keycast-Setup.exe` + `Install-Keycast.bat` + `Keycast.cer`) to the target PC.
2. Run **`Keycast-Setup.exe`** (or double-click **`Install-Keycast.bat`** to pre-trust the certificate and bypass SmartScreen warnings with one click).
3. Follow the wizard. The installer automatically:
   - Installs Keycast to `%LOCALAPPDATA%\Programs\Keycast` (no administrator privileges required).
   - Creates a Start Menu shortcut and optional Desktop shortcut.
   - Registers the digital code signing certificate in the Windows User Trusted Store so Windows Security and SmartScreen verify the binary.
   - Unblocks Mark-of-the-Web (Zone.Identifier) alternate data streams.
   - Provides a clean uninstaller in Windows Settings / Add or Remove Programs.
4. Launch **Keycast** from the Start Menu or Desktop.

### Option B: Portable Mode
1. Extract `Keycast-Windows.zip`.
2. Run **`Trust-Certificate.bat`** once (or **`Run-Keycast.bat`**) to trust the certificate and unblock the executable.
3. Open **`keycast.exe`**.

## How to Broadcast and Receive

1. On the main PC, leave **This is a receiving PC** unchecked. Click **Show**, select the pairing code and copy it. Treat this code as a secret. The code is generated afresh each launch and is not saved.
2. On each other PC, check **This is a receiving PC**, paste the main PC's code, and click **Enable receiver**.
3. Keep all PCs on the same LAN/Wi-Fi. If Windows Firewall asks, allow Keycast on your trusted **Private** network. The app does not alter firewall rules.
4. On the main PC, click **Find my PCs**. Receivers appear automatically, usually within two seconds. Check the desired PCs and wait for **Paired / ready**.
5. Release any held modifier keys, then click **Start broadcasting**. Focus the desired app on each receiver and your main PC. Begin typing.
6. **F12 pauses immediately on the PC where it is pressed.** On the main PC it stops broadcasting to all selected receivers. On a receiver it disables incoming input and ends its connection. F12 never travels over the network, but still reaches local apps normally. Resume with the button; after a receiver pauses, re-enable it and reselect it on the main PC.

Changing selected PCs pauses the group. Restart Keycast to change mode or pairing code. Keep the app running; minimizing is supported, closing stops it. There is no background service, tray mode, startup registration, or silent auto-resume.

## What to expect

- Automatic IPv4 LAN discovery with a receiver checklist and pagination.
- Independent authenticated TCP connections for each selected receiver.
- ChaCha20-Poly1305 encrypted key traffic using a random 128-bit pairing code.
- Fresh receiver challenges and ordered sequence numbers reject replayed frames.
- Bounded queues; overload pauses the sender instead of silently dropping key-up events.
- Held keys are released on pause, deselection, disconnect, normal exit, and a one-second receiver heartbeat timeout, provided Windows permits input injection.
- No keystroke logging, cloud service, or internet account. Discovery exposes the PC name and local IP to the LAN; keyboard traffic is encrypted.

## Limitations

This is an initial build, not a guarantee of exact simultaneity. Wi-Fi, Windows scheduling and target apps introduce variable delays. Each PC receives the same physical keys, not guaranteed identical text.

- Use the same keyboard layout and synchronize Caps Lock / Num Lock on every PC before starting. Existing modifier state is not synchronized; release modifiers before starting/resuming. IME, touch keyboards, clipboard contents, and mouse events are not synchronized.
- Ctrl+V pastes each PC's own clipboard. Receiver-local keyboard activity can conflict with remote modifiers, so avoid typing locally while receiving.
- Windows secure desktops, lock screens, UAC prompts, Ctrl+Alt+Delete, and some games are not supported. A normal receiver cannot type into a higher-privilege app. See [Microsoft SendInput documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).
- F12 is reserved for pause. It is not a toggle, and injected/synthetic key events are ignored to prevent forwarding loops.
- Keys already in flight may arrive before the pause/release message; no network program can retract input already delivered. If Wi-Fi is lost, the receiver releases held keys after its heartbeat deadline.
- Force-killing the receiver or losing power prevents cleanup. Windows can also refuse release events when focus switches to a protected desktop.
- Discovery uses IPv4 broadcast and may be blocked by guest Wi-Fi, client isolation, VPN routing, multiple adapters, VLANs or firewall policy. There is no manual-IP fallback yet.
- Only one active sender is accepted per receiver. One receiver instance per PC is supported. All PCs sharing the code can authenticate, so use a fresh code if it has been disclosed.
- The executables and installer are digitally signed with an Authenticode SHA-256 certificate and embed full Windows PE version information, icons, and modern High-DPI/Windows 10/11 compatibility manifests.

## Troubleshooting

| Symptom | Check |
|---|---|
| No PCs found | Receiver enabled, same LAN, Private firewall permission, no client isolation |
| Pairing fails | Exactly the same 32 hex characters on every PC; receiver enabled and not busy |
| Connection drops after F12 on receiver | Re-enable receiving, reselect that PC, then resume broadcasting |
| Input missing | Correct focused window, matching layouts, matching privilege levels |
| Network error on startup | Another receiver may already be using the ports; close it and restart |

Network ports: UDP **45832** for discovery, TCP **45833** for encrypted keyboard sessions. Do not expose these ports to the internet. No firewall or router changes are made automatically.

## Build and test

Install stable Rust for `x86_64-pc-windows-msvc` and Visual Studio C++ Build Tools with the Windows SDK. From `source`:

```powershell
cargo build --release --locked
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

The app uses native Win32 controls, a low-level keyboard hook and Windows `SendInput`. The hook only queues events and never performs network I/O. Synthetic input is ignored. Tests inject into a recording backend, not your desktop.

Modules: `ui.rs` (native UI), `input.rs` (hook and key lifecycle), `network.rs` (discovery and sessions), `protocol.rs` (authenticated framing), `state.rs` (pause epochs and shared state).

## Validation

The release build and Clippy checks pass. Twelve unit tests pass, covering authentication, tampering, session/sequence replay, frame limits, fragmented frames and held-key tracking. Four loopback TCP tests are included for cleanup after pause, release, disconnect and heartbeat timeout, but could not run here: the sandbox rejects socket access with Windows error 10013. They remain enabled in the normal test suite. The non-network subset can be run with `cargo test --locked -- --skip tcp_`.

A real multi-PC Wi-Fi trial is still required. This environment has only one accessible PC and no working UI inspection helper, so window appearance, actual physical-key capture and remote typing have not been manually verified.

Suggested acceptance test: open a blank text editor on two receivers, broadcast letters, Shift+letters, arrows, Backspace and a held key; pause with F12 and verify subsequent typing stays local. Repeat while unchecking a receiver and disconnecting its Wi-Fi. Start with non-sensitive text.

