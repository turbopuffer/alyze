---
type: guide
tags: [ wiki, guide, how-to, contributing ]
title: Become an effective contributor
description: "What actually gets a PR merged into turbopuffer/alyze: the system-level rubric, the reviewer cast, and the lanes that work, from real review history."
---

## Goal

Get a change merged into the real project, `turbopuffer/alyze` on GitHub, not just written correctly. This repo (`aatran14/alyze`) is a personal fork that a scheduled job merges `turbopuffer/alyze`'s trunk into every day automatically; nothing here gets independently reviewed. Contributing for real means opening a PR against `turbopuffer/alyze` itself.

Everything below is drawn from that repo's actual merged and closed pull requests and their review threads, not general advice. The whole page compresses to one sentence: an effective contribution is either something a reviewer already asked for, or something that strengthens what alyze is for at a cost the system doesn't care about; the rest of this page derives that from the evidence.

## Start from what alyze is for

alyze has one job in turbopuffer: turn text into terms so a query and a document agree on what counts as the same word. It runs in two places, and the two placements price every resource a change might spend or save.

On the **write path**, each document is tokenized once at ingest. The cost is fleet CPU: real money at scale, but amortized, and nobody is waiting on it.

On the **read path**, strong consistency means a query cannot just search the prebuilt index; recent writes that haven't been indexed yet get re-tokenized at query time, inside the query's latency budget. The same query is also fetching index bytes from object storage, which costs hundreds of milliseconds cold, while tokenizing the small unindexed tail costs nanoseconds per word.

From that placement, each resource has a very different price:

- **Latency**: nearly worthless to win. I/O dominates what a user feels; the tokenizer's share of a cold query is a rounding error. See the napkin math in [Performance philosophy](../architecture/performance-philosophy.md).
- **Memory**: the most expensive thing to spend. Per-analyzer state (like the stemming cache) multiplies across every namespace and concurrent query on a node.
- **CPU**: free money to save. Tokenization runs on every write and every consistent read, so genuinely free reductions cut fleet cost with no downside.
- **Determinism**: absolute. Query-time analysis must byte-match write-time analysis indefinitely, so a shortcut must be provably identical, never approximately right.

Morgan Gallant (the maintainer) evaluates perf work against this rubric, not against benchmark numbers. This is why he is against SIMDmaxxing the crate: heroic machinery (SIMD, inlining tricks, bigger caches, fused stages) spends complexity or memory on latency, the dimension worth the least. It is not a ledger where a big enough win pays for complexity; some wins are simply uninteresting at the system level, and any machinery they require is pure cost. What stays interesting is removing redundant work at near-zero cost: "least work for identical output," not MiB/s-maxxing.

## The process, as it actually exists

There is no `CONTRIBUTING.md`. The process is: open a PR, and Morgan Gallant (the sole name in `.github/CODEOWNERS`, and the author of nearly every commit) reviews it. Two mechanical checks have to pass either way: `cargo fmt --all --check` and `cargo test --workspace --exclude alyze-wasm` (`.github/workflows/ci.yml`).

The reviewers are not interchangeable; each shows up with a consistent lens, visible across every comment they've left:

- **`morgangallant`** (maintainer): decides direction, applies the rubric above. Merges perf wins that arrive free (#15, #17); declines wins that spend memory or complexity (#16, #22). Also the one who parks decisions; see Gotchas.
- **`jpountz`** (Lucene committer; the most substantive technical reviewer): compares against Lucene's design, pushes back on "it's difficult" claims, and repeatedly asks for the *why* to be written down at the point of use. On PR #27 alone: "add a comment to explain why you're using this feature", "why `inline(always)` vs. just `inline`?", "is this a breaking change?". If a choice is non-obvious, comment it before he asks.
- **`pushrax`** (tpuf founder): API legibility. His entire review of PR #10 was asking for doc comments ("useful spot to put a doc comment on what 'word like' means").
- **`benesch`** (tpuf eng): clean integration into the turbopuffer build; authored the `tpuf-vendored` feature (PR #8) so turbopuffer avoids a duplicate icu_properties. Mostly approves quietly.
- **`mattcuento`**: test coverage. Asked for the remaining validation cases on PR #19, and on PR #20 asked for β/Σ byte-length tests "and maybe stemming/ascii folding as well"; Morgan's response was "Love these, added." His asks are a standing to-do list.
- **`mlpuff`**: the bindings' fix-forward voice; approved the Python SDK with "looks good as a v0 ship and then let's fix forward whatever falls out", and left concrete nits (`uv pip install maturin`) that are still open work.

## What gets merged vs. rejected, with real examples

- **PR #17, "perf: add HAS_ASCII_UPPER to TokenProperties"** (merged): added one new bit to the existing `TokenProperties` byte, gating a pipeline stage that already existed. A CPU reduction at zero cost on every other dimension. Morgan's merge comment: "Lovely, thanks for doing this!"
- **PR #15, "perf: optimize english stopword lookup"** (merged): replaced a hash lookup with a direct byte matcher, +27.4% on the stopwords bench. Same shape: less work, nothing spent.
- **PR #16, "perf: expand stemming cache"** (rejected): a real, measured win (+29% on stemming, +26% full pipeline) bought with about 8 MB more memory per buffer. That trades the system's most expensive resource for its least valuable one, so the size of the win never mattered. Morgan: "32k + 10 byte max was somewhat deliberately chosen to be a reasonable balance between throughput and memory usage."
- **PR #22, "perf: make the no-filter analyze path actually inline"** (rejected): an even bigger measured win (+27% tokenize-only, +4% full pipeline) using const generics and inlining tricks to fuse closures into the hot loop. Complexity spent in the most correctness-critical code, on the worthless dimension: "I worry a bit about the code complexity this adds. Although there's a sizeable performance win (amazing!), probably going to bias towards not merging this guy for the sake of simplicity."
- **PR #18, "perf: skip lowercase rescan via tokenizer-computed uppercase bit"** (closed, not rejected on merit): nearly identical to PR #17, which landed first. Closed amicably as a duplicate, with a thank-you either way.

Every one of these is the rubric applied. A bigger benchmark number is not a stronger argument; where the win lands and what it spends is the whole argument.

## The three lanes that work

1. **Work a reviewer already asked for.** Review comments are pre-validated demand: `mattcuento`'s test asks (#19, #20) and `mlpuff`'s uv nit (#24) are explicit requests nobody has picked up. The [bindings work handoff](./bindings-work-handoff.md) is the current graded list of this kind of work.
2. **Tests and doc comments.** The project's core promise is deterministic, byte-identical output (Morgan divergence-tested against WordV3 and added a golden hash suite in PR #27). Tests strengthen that promise while spending nothing on any dimension of the rubric, so they can't trip the veto. Doc comments are what `pushrax` and `jpountz` repeatedly ask for anyway. For an outside contributor, these lanes are where value and acceptability fully overlap.
3. **Perf in the #17 shape.** A CPU reduction that reuses existing state (a new bit in an existing struct beats a new cache, generic parameter, or inlining trick), provably identical output, benchmark numbers in the description, and its own costs stated up front.

## Steps

1. **Mine the PR history for what reviewers already asked for.** Grade any candidate item by its evidence: reviewer-requested beats verified-gap beats pure suspicion, and drop anything history shows was already declined.
2. **Check open PRs.** #18 was closed purely because it duplicated #17; a quick look would have caught that before writing any code. Open PRs also telegraph incoming churn; a big refactor in flight (like #27) will move the code you're about to cite or test.
3. **Read [Principles this repo won't compromise on](../architecture/principles.md)** before writing anything. It's the same bar these real PRs were held to, just written down in advance.
4. **Ask the rubric's question before the bench's question.** "Is this win interesting at the system level?" comes before "does it bench well?". If the win lands on latency, assume it's not interesting until argued otherwise.
5. **Bring a real benchmark number anyway.** Every perf PR in this history, accepted or rejected, included actual `cargo bench` output in its description. Table stakes, not a nice-to-have.
6. **State your own cost up front.** PR #16 quoted its own memory cost ("+8.15 MiB total") before anyone asked. Naming your own tradeoff reads as good judgment, even on a PR that still gets turned down.

## Gotchas

- **Some things are parked on purpose; check before building.** npm/PyPI publishing looks like backlog ("not published" in both READMEs) but is a stated decision: Morgan on PR #25, "probably best to have this unpublished for the time being." Same for the 32k/10-byte stemming-cache budget (PR #16). Building against a parked decision wastes the work no matter how good the PR is; when in doubt, ask in an issue first.
- **Rejection here is not adversarial.** Both rejected PRs above got specific, warm thanks and explicit encouragement to keep contributing, not a cold close. The actual risk of proposing something and being told no is low; don't self-censor ideas because of it.
- **This fork has no review of its own.** Any change made only here, without a PR to `turbopuffer/alyze`, will be silently overwritten the next time the daily sync job runs.

## Related

- [Principles this repo won't compromise on](../architecture/principles.md)
- [Performance philosophy](../architecture/performance-philosophy.md)
- [Workspace & crate boundaries](../architecture/workspace-boundaries.md)
- [Work handoff: bindings cleanup tasks](./bindings-work-handoff.md)
- [Reproduce the throughput benchmarks](./reproduce-benchmarks.md)
