# WitnessTx strategy (Phase 0 note / F3)

## Context

RGB verification today is typed against `bitcoin::Transaction` in
`rgb-consensus`. Liquid (Elements) transactions cannot deserialize as that type
because of confidential amounts, per-output asset ids, and fee outputs.

[kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike)
introduces a small **`WitnessTx` trait** (~207 LOC) so verification only needs:

- transaction id  
- input outpoints (seal closure)  
- output `scriptPubKey`s (tapret / opret commitment recovery)

`bitcoin::Transaction` implements the trait unchanged; an Elements/Liquid
transaction implements the same three methods.

RFC / upstream: [rgb-protocol/rgb-consensus#12](https://github.com/rgb-protocol/rgb-consensus/issues/12)
(see spike `RFC.md`).

## Decision for `rgbmvp` (implemented in P0)

| Item | Choice |
|------|--------|
| Vendor path | `vendor/rgb-consensus-patched/` (Apache-2.0, spike PATCH.md) |
| Workspace | `[patch.crates-io] rgb-consensus = { path = "vendor/rgb-consensus-patched" }` |
| RGB pin | `rgb-ops` / `rgb-schemas` / `rgb-consensus` **=0.11.1-rc.10** |
| Liquid adapter | `lab_rgb::seal::WitnessTx` implements `rgbcore::dbc::WitnessTx` (from Esplora JSON) |
| LWK | Seals, PSET, broadcast; commitment via **unconfidential** P2TR (`add_explicit_recipient`) |
| `elements` | 0.25.x aligned with LWK 0.18 |

## Live evidence

- Patched `Anchor::verify` accepted a real Liquid Testnet witness
  (`2b1b2f045ab9797ff34dd919293fb5b67e4e123d5557d99fc90fd06cb36c2635`).
- Unit tests in `lab-rgb` (issue/transfer/mpc/dbc) pass offline.

## Remaining risks

- LWK may reorder outputs; TapretFirst needs the commitment among P2TR outs
  (we observed commitment at `vout=0` in the live demo).
- Full RGB consignment validation (history / AluVM) is still thin vs production wallets;
  P0 proves **issue + transfer plan + seal close + anchor verify** on Liquid Testnet.
