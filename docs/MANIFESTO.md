# Manifesto — RGB on Liquid, for Bitcoin

**Project:** [rgbmvp](https://github.com/EdwinKestler/rgbmvp) · mirror [ffwd-org/rgbmvp](https://github.com/ffwd-org/rgbmvp)  
**Audience:** Bitcoin builders, Liquid operators, RGB researchers, wallet and exchange engineers  
**Stance:** Public lab · testnet-first · no mainnet claims without review  

---

## Why this exists

Bitcoin is settlement-grade money. Liquid is a Bitcoin sidechain optimized for **fast settlement, confidential amounts, issued assets, and—now—programmable covenants (Simplicity)**. RGB is **client-side validated** contract history: the chain only sees a **single-use seal** and a **commitment**, not the full state machine.

For years, RGB’s public story was strongest on **Bitcoin**. Liquid’s strengths—**confidential transactions, low fees, asset tooling (LWK), and Simplicity**—were under-exercised as a **first-class RGB anchor layer**. Cross-chain “move my RGB token to Liquid” often meant **bridges or confusion with native Liquid assets**.

**We refuse that confusion.**

This project exists to make three ideas **demonstrable in public**, not merely aspirational:

1. **RGB can live natively on Liquid** — issue, transfer, and **client-side verify** with real anchors on Liquid Testnet.  
2. **Interop without a custodian** — value crosses Bitcoin ↔ Liquid by **atomic swap of twin contracts / HTLC legs**, not by teleporting one contract id.  
3. **Liquid’s unique power is usable with RGB** — Simplicity can **enforce seal policy on-chain**; backing terms can live **in the contract id** and be audited **without an oracle**.

If Bitcoin is the root of trust and Liquid is a high-performance workshop, **rgbmvp is a public workbench** where the community can see the tools fit together.

---

## What we are building (and what we are not)

### We are building

| Layer | Contribution |
|-------|----------------|
| **Protocol lab** | End-to-end paths: RGB-on-Liquid, twin HTLC swap, Simplicity seal covenants, BFA audit |
| **Shared machine API** | Browser and CLI share **`/v1` JSON** so validation is not forked into UI code |
| **Operator-safe demos** | Testnet fixtures, redacted preimages, no seeds in the browser |
| **Reference implementation** | Phased scenarios, closed-phase evidence, headless crates + scripts for reproduction |
| **Upstream-aligned stack** | LWK for Liquid wallets; vendored `WitnessTx` patch path toward multi-chain RGB consensus |

### We are not building (yet / by design)

- A mainnet product or “trust us” bridge  
- A claim that **native Liquid assets** are RGB  
- A Lightning-required architecture for core demos  
- A full non-custodial browser wallet that holds user seeds  

Honesty is part of the product. **Closed phases** (`docs/*_CLOSED.md`) record what was proven and what was deferred.

---

## Innovation we care about

### 1. RGB anchored on Liquid, verified like Bitcoin

RGB’s history is off-chain; the chain only enforces **seal closure** and **commitment integrity**. Making that path work on Liquid requires treating Liquid transactions as first-class witnesses (the **`WitnessTx`** direction) and building real **issue → transfer → verify** against Liquid Testnet explorers and LWK.

**Why it matters:** Liquid becomes a place where RGB assets can settle with **faster blocks, CT privacy for the seal economy, and mature wallet tooling**—without inventing a second token standard that pretends to be RGB.

### 2. Twin-contract atomic swap (no bridge black box)

A contract’s genesis is bound to **one** chain net. “Moving” value to another chain means **swapping ownership of two assets** under a shared secret and timeout—**HTLC**—optionally documented with twin `rgb:` ids.

**Why it matters:** Users and venues can rebalance **BTC-testnet RGB / value ↔ Liquid RGB / L-BTC** without depositing into a custodian. Failure modes (refund after CSV) are explicit.

### 3. Programmable seals (Simplicity) + oracle-free backed assets (BFA)

- **Simplicity covenants** can require that a seal spend **also** carries an RGB-shaped commitment (and, for mint-gates, **exact vault backing + gate recursion**)—enforced by **consensus**, not only by polite clients.  
- **BFA** commits **vault, backing asset, and rate** into genesis so every holder can **rebuild mint history** and reject under-backing or a forged history **without an oracle**.

**Why it matters:** This is Liquid’s **differentiator**. Bitcoin Script is weak at rich covenants; Liquid + Simplicity can make “invalid for RGB” and “invalid for the chain” **align** for critical moves.

### 4. Lab console without browser keys

A thin web UI over **`/v1`** lets operators and third parties **see and drive** demos while **keys and preimages stay on labd/CLI**.

**Why it matters:** Public demos stop being “trust this AppImage” or “parse this 40-line command.” They become **shareable, inspectable flows**.

---

## Goals for the community

1. **Make RGB-on-Liquid a default mental model**, not a footnote.  
2. **Normalize twin-swap language** so “cross-chain RGB” does not imply a magical bridge.  
3. **Exercise Simplicity with RGB seals** so covenant research has a public, runnable reference.  
4. **Show oracle-free backing audit** as a building block for stablecoins, funds, and reserves on Liquid.  
5. **Leave a ladder others can climb**—scenario IDs, closed evidence, headless demos, and an agent-friendly protocol ([M2M.md](./M2M.md)).  
6. **Stay testnet-first** so the community can break and improve the stack without mainnet blood.

---

## Real-life use cases

These are **design targets** the lab is built to illuminate. The repository demonstrates **protocol pieces** on testnet; production requires wallets, compliance, and upstream maturity.

### A. Private settlement of RGB invoices on Liquid

**Who:** merchants, OTC desks, payroll in tokens.  
**Flow:** issue/receive RGB on Liquid; pay fees in L-BTC; verify consignments client-side.  
**Lab touchpoint:** P0 issue/transfer/verify · browser Issue/Transfer/Verify · LWK-backed wallets.

### B. Fast rebalancing between Bitcoin and Liquid inventories

**Who:** market makers, exchanges, Lightning service providers holding RGB or L-BTC inventory.  
**Flow:** atomic HTLC swap of BTC-side value for Liquid-side value (twins documented with contract ids).  
**Lab touchpoint:** P1 / P3 guided swap · `btc-alice` ↔ `bob` · preimage redacted in UI.

### C. Peg-style synthetic assets without a trusted mint API

**Who:** stablecoin and RWA experiments on Liquid.  
**Flow:** inflation-gated mint only when **backing** is locked (or burned); holders audit full history from genesis terms.  
**Lab touchpoint:** P2 C1 mint-gate · C3 BFA audit (CLI + `/audit` page).

### D. Covenant-enforced “don’t orphan the RGB seal”

**Who:** swap protocols, vaults, automated market infrastructure.  
**Flow:** seal UTXO may only be spent if the spend also posts an **RGB-shaped anchor** (and optional hashlock).  
**Lab touchpoint:** P2 C0 Simplicity `preimage ∧ opret` · regtest demos.

### E. Permissionless but rule-bound mint windows

**Who:** community funds, points programs, reserve-backed credits.  
**Flow:** anyone may mint if they lock the tranche and re-create the gate—**no issuer key on the gate**.  
**Lab touchpoint:** C1 mint-gate recursion demos.

### F. Public education, RFCs, and wallet integration

**Who:** wallet teams (LWK/WASM), RGB implementers, Liquid Federation ecosystem.  
**Flow:** treat this repo as a **reference ladder**: scenarios, WitnessTx notes, API shapes, and known failure modes.  
**Lab touchpoint:** entire monorepo · [HEADLESS.md](./HEADLESS.md) · [ARCHITECTURE.md](./ARCHITECTURE.md).

### G. Agent-assisted open source

**Who:** multi-agent coding systems and CI assistants.  
**Flow:** discover code via optional project memory; still edit files as truth; never put secrets in Redis.  
**Lab touchpoint:** [M2M.md](./M2M.md) · [PROJECT_MEMORY.md](./PROJECT_MEMORY.md).

---

## How this repository serves as a starting point

| You want to… | Start here |
|--------------|------------|
| Understand the vision | **This manifesto** |
| Run a 15-minute human demo | [README.md](../README.md) · [PURPOSE_AND_USAGE.md](./PURPOSE_AND_USAGE.md) |
| Reproduce protocol claims without UI | [HEADLESS.md](./HEADLESS.md) · `scripts/demo_c0|c1|c3_*.sh` |
| Integrate a wallet or service | [ARCHITECTURE.md](./ARCHITECTURE.md) · `GET /v1` · `crates/lab-*` |
| See what is already proven | [P1_CLOSED.md](./P1_CLOSED.md) · [P2_CLOSED.md](./P2_CLOSED.md) · [P3_CLOSED.md](./P3_CLOSED.md) |
| Extend the ladder | [SCENARIOS.md](./SCENARIOS.md) · map new work to scenario ids |
| Automate with agents | [M2M.md](./M2M.md) · [AGENTS.md](../AGENTS.md) |

Clone it. Break it on testnet. Cite the closed docs when you claim a property. Upstream what belongs in `rgb-consensus`, LWK, and Elements—not only in a lab folder.

---

## Principles we hold

1. **Bitcoin remains the root.** Liquid and RGB are tools for scaling *use* of Bitcoin’s security model, not replacements for it.  
2. **Client-side validation is a feature.** Privacy and scale come from not putting full state on-chain—**if** seals and commitments are sound.  
3. **Chains enforce what they can; clients enforce what they must.** Covenants close honesty gaps; schemas and audits close the rest.  
4. **No false bridges.** If it is not atomic and twin-bound, do not call it “the same RGB asset on another chain.”  
5. **Public demos over private slide decks.** Runnable scripts beat whitepapers alone.  
6. **Testnet is where trust is earned.** Mainnet waits for review, pins, and sober ops.  
7. **Open reference over closed product theater.** Prefer shared `/v1`, clear scenario ids, and reproducible evidence.

---

## Call to the Liquid and Bitcoin communities

- **Liquid builders:** use this lab when you need RGB + LWK + (optional) Simplicity in one place.  
- **RGB builders:** treat Liquid as a peer anchor network, not an afterthought.  
- **Wallet vendors:** take the `/v1` shapes and seal/verify flows as integration sketches—not final product UX.  
- **Researchers:** fork the covenants and BFA audit; challenge them; improve them; send patches upstream.  
- **Educators:** show students a **full ladder** from faucet L-BTC to valid RGB verify to dual-chain swap.

We are not asking anyone to “believe in RGB on Liquid.”  
We are asking them to **run it**, **break it**, and **build the next layer** with clearer language and harder guarantees.

---

## Closing

**Bitcoin** is the base money.  
**Liquid** is a workshop for speed, privacy, and programmable settlement.  
**RGB** is client-side contract logic that can ride either rail.  

**rgbmvp** is a public workbench where those three meet—openly, on testnets, with proofs you can re-run.

*If you only take one sentence from this manifesto:*

> **Cross-chain RGB is atomic twins, not a bridge; Liquid is a first-class RGB home; covenants and oracle-free audit are how Liquid makes that home safer.**

---

## Links

| Resource | URL / path |
|----------|------------|
| Repository | https://github.com/EdwinKestler/rgbmvp |
| Org mirror | https://github.com/ffwd-org/rgbmvp |
| Human usage | [PURPOSE_AND_USAGE.md](./PURPOSE_AND_USAGE.md) |
| Agent protocol | [M2M.md](./M2M.md) |
| KaleidoSwap spike | https://github.com/kaleidoswap/rgb-on-liquid-spike |
| LWK | https://github.com/Blockstream/lwk |
