# Security Policy

`rgbmvp` is a **research lab** for RGB-on-Liquid, Bitcoin/Liquid HTLC swaps, and
Simplicity seal demos. It runs on **Liquid Testnet / Bitcoin testnet / regtest
only** — mainnet is refused at configuration load (`lab-core`, `lab-btc`), and
the code is not intended to custody real value.

We still take reports seriously: a flaw here could mislead someone into trusting
an unsound proof, or become dangerous if this code were ever adapted toward
mainnet. Responsible disclosure is welcome.

## Reporting a vulnerability

**Please do not open a public issue for security reports.**

Use GitHub's private reporting instead:

1. Go to the **Security** tab of this repository.
2. Click **Report a vulnerability** (GitHub Private Vulnerability Reporting).

This opens a private advisory visible only to the maintainers. If you cannot use
that channel, open a minimal public issue asking for a private contact — without
technical detail — and we will follow up.

Please include, where you can:

- affected component (crate/path, endpoint, or CLI command),
- version or commit (`git rev-parse HEAD`),
- a reproduction (inputs, expected vs. actual), and
- impact assessment.

### What to expect

- **Acknowledgement:** within 5 business days.
- **Assessment:** we trial-triage and confirm/deny, keeping you updated.
- **Fix & disclosure:** coordinated; we will credit you unless you prefer
  otherwise.

## In scope

- The `labd` HTTP server (`crates/lab-cli`, `crates/lab-api`) — auth, path
  handling, CORS, headers, rate limiting, and any way to reach a mutating
  endpoint without a valid token in public read-only mode.
- **T1 bounded demo swaps** (`POST /v1/demo/swap`, `lab_core::demo`,
  `lab_core::secrets`) — this is the only endpoint that lets an unauthenticated
  visitor cause a state change, and only when `LABD_DEMO_SWAPS=1`. Especially
  in scope: bypassing the Turnstile check, the per-IP/daily/concurrency quotas,
  the fee budget, or the wallet float floors; influencing swap parameters
  (amounts, fees, wallets, CSV delay) which must be server-fixed; reaching any
  other mutating endpoint through the demo exemption; and anything that lets
  the refund watcher act on a swap it did not create.
- Protocol/soundness bugs: HTLC scripts (`crates/lab-rgb/src/htlc.rs`), seal
  and DBC commitment verification (`vendor/rgb-consensus-patched/`), Simplicity
  programs (`crates/lab-simplicity`, `programs/`), and BFA audit logic.
- Preimage/secret leakage on any **public** surface.
- Dependency vulnerabilities (tracked via Dependabot alerts and the CI
  `cargo audit` job).

## Out of scope (by design, not bugs)

- **Testnet key material is public on purpose.** `fixtures/testnet_wallets.json`
  contains BIP39 phrases published for reproducible tests, and demo HTLC
  keypairs are derived deterministically from labels
  (`htlc.rs::demo_keypair`). These are testnet-only and hold no real value.
- **T1 runs custodially by design.** When enabled, the server holds spendable
  testnet keys and swaps between its own wallets. That the operator can spend
  the demo float is the design, not a vulnerability; the float is small and
  capped. Escaping the caps, or extracting the keys, **is** in scope.
- **Mainnet is intentionally unreachable.** Reports that require enabling
  mainnet are out of scope; if you find a way to *bypass* the mainnet refusal,
  that **is** in scope.
- Operator-run deployment secrets, faucet balances, and third-party testnet
  explorer availability.
- Denial of service from unrealistic request volumes against a single-instance
  demo (the demo is rate-limited and capacity-capped, not hardened for load).

## Supported versions

This is a moving research repository; only the tip of `main` is supported.
Fixes land on `main` and are not backported.
