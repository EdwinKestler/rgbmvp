# Public artifacts (no secrets)

Canned **read-only** evidence for the public demo board and static hosts (Vercel).

| File | Contents |
|------|----------|
| [`manifest.json`](./manifest.json) | Phase chips P0–P3, S3, C0–C4, U4 + explorer links |
| [`s3-rgbmvp-live.json`](./s3-rgbmvp-live.json) | Live S3 session summary (**preimage redacted**) |

**Never** put seeds, WIFs, mnemonics, or preimages here.

Regtest covenant txs are **local Elements** ids (not Blockstream explorers). Re-run demos for fresh hashes:

```bash
./scripts/demo_c0_simplicity.sh
./scripts/demo_c1_mint_gate.sh
./scripts/demo_c2_mint_gate_burn.sh
./scripts/demo_c3_bfa_audit.sh
./scripts/demo_c4_stake.sh
```
