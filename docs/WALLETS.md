# Local wallets for Liquid lab work

This project uses **programmatic wallets** (LWK / later lab CLI) for automation.
For human testnet UX and sanity-checking Liquid addresses, balances, and
**native** asset swaps, a desktop wallet is useful.

## SideSwap (Linux AppImage)

| | |
|--|--|
| **What** | Non-custodial Liquid wallet + P2P atomic swap marketplace |
| **Site** | [sideswap.io](https://sideswap.io/) · [downloads](https://sideswap.io/downloads/) |
| **Source** | [sideswap-io/sideswapclient](https://github.com/sideswap-io/sideswapclient) (Flutter/Dart + Rust core; GPL-3.0) |
| **Rust core** | [sideswap-io/sideswap_rust](https://github.com/sideswap-io/sideswap_rust) |
| **Stack** | [Blockstream GDK](https://github.com/Blockstream/gdk) for Liquid wallet primitives |
| **Local binary** | `SideSwap.AppImage` in the repo root (**gitignored**, ~20 MB) |

### What SideSwap is good for in `rgbmvp`

- Create / import a Liquid mnemonic and receive **testnet L-BTC** (faucet → address).
- See confidential balances, send/receive **native Liquid assets** (L-BTC, USDt, AMP, etc.).
- Experience Liquid’s **native atomic swaps** (order book / P2P) as a *contrast* to our **RGB twin-swap** path (P1).
- Manual cross-check that lab-generated addresses and on-chain activity look right on an explorer.

### What SideSwap is **not**

- **Not an RGB wallet.** It will not issue, transfer, or validate RGB consignments.
- **Not a substitute for LWK** in the lab stack (LWK is the library we embed for PSET/CT automation).
- Peg-in/out and mainnet markets are for real Liquid; our public lab stays on **Liquid Testnet**.

### Testnet mode

Per SideSwap FAQ ([testnet](https://sideswap.io/faq/testnet/)):

1. Open SideSwap → **Settings**.
2. Switch the environment selector to **Liquid Testnet** (testnet wallet is derived from the same seed as mainnet).
3. Testnet web UI: [testnet.sideswap.io](https://testnet.sideswap.io/).
4. Fund L-BTC from [liquidtestnet.com/faucet](https://liquidtestnet.com/faucet).
5. Explorer: [blockstream.info/liquidtestnet](https://blockstream.info/liquidtestnet/).

### Run the local AppImage

```bash
cd /path/to/rgbmvp
chmod +x SideSwap.AppImage
./SideSwap.AppImage
```

Optional: verify the vendor release with their [PGP key](https://sideswap.io/resource/sideswap.gpg.txt) if you re-download from GitHub releases.

**GUI notes:** needs a graphical session (X11/Wayland). Headless servers cannot use the AppImage interactively.

### Observed local issues (2026-07-21)

On this host, `./SideSwap.AppImage --appimage-extract-and-run` started then failed with:

- `loading utxos failed: the AMP wallet is disconnected` — AMP subsystem not connected (often mainnet AMP path; switch to **Liquid Testnet** in Settings if the UI stays up).
- Flutter / OpenGL teardown: `Couldn't find current GLX or EGL context` — graphics stack mismatch (Wayland/X11, GPU drivers, remote session).

**Workarounds:**

1. Prefer a normal desktop session (local X11 or working Wayland GL).
2. Try: `QT_QPA_PLATFORM=xcb ./SideSwap.AppImage` or run under a full desktop, not SSH without GL.
3. For lab funding, **prefer Phase 0 CLI** (`rgbmvp wallet create` / `address`) + [liquidtestnet.com/faucet](https://liquidtestnet.com/faucet) — SideSwap is optional human UX, not a build dependency.


### Security

- Treat the mnemonic as a secret; do not put it in git, project memory, or lab Redis.
- Prefer a **throwaway testnet-only** seed for lab work; never reuse mainnet funds on the same machine habits as testnet demos if you can avoid it.
- The AppImage is **not** part of the reproducible lab build; keep it local (see `.gitignore`).

### Relation to the architecture

```text
SideSwap (human)     →  native Liquid CT assets, P2P Liquid swaps, faucet UX
LWK (lab libraries)  →  same Liquid layer, automated PSET / issue / broadcast
lab-core (RGB)       →  RGB contracts anchored on Liquid UTXOs (separate layer)
```

See [ARCHITECTURE.md](./ARCHITECTURE.md) and [STACK.md](./STACK.md).

For **rgbmvp CLI fixture wallets** (alice/bob/carol/maker) and the P0 fund trail, see [TESTNET_WALLETS.md](./TESTNET_WALLETS.md).
