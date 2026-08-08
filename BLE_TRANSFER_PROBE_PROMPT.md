# Prompt: measure what the watch's BLE File Transfer Service can actually carry

You are establishing one number and one protocol: **how fast can a desktop push a file to the
UNA watch over BLE, and what is the framing that does it?**

Everything downstream of this is currently guessing. A 3 MiB map pack takes **0.9 minutes or
20 minutes** depending on the answer, and that gap decides whether the product is phone-first
or desktop-only. It also decides whether a wire-format change to `rawtiles` is needed at all.
Nobody can design around this until it is measured.

The read path is already reverse-engineered, working, and validated against real hardware.
**You are extending a working client, not starting from scratch.**

---

## 0. Ground rules

- **Never post anything to GitHub** — no PRs, comments, or issues, on any repo. `gh` is
  read-only. Deliverables are local commits on a branch, pushed to `origin` only.
- **Verify, don't trust this prompt.** Every claim below was checked on 2026-08-08 against
  `una-sdk` branch `investigate/ble-large-file-transfer`. Re-verify before acting.
- Label findings **`CONFIRMED`** (observed on hardware) or **`PLAUSIBLE`** (reasoned). For
  each `PLAUSIBLE`, say what would settle it.
- **Report timings honestly, including the bad ones.** A slow result is the finding. Do not
  quietly retry until you get a fast number and then report that one — report the
  distribution and say how many runs it came from.
- Evidence goes in `una-sdk/Docs/Investigations/<date>-<slug>/` — hypothesis, method, log,
  verdict. Raw timing data committed, not just summarised.
- Commits terse, mostly *why*. Branch off `investigate/ble-large-file-transfer`.

## 0.1 SAFETY — read this before you write a single byte to the watch

You are about to use an undecoded write protocol against a device's filesystem. Three ways
to do real damage, in descending order of how bad:

1. **`0:/ble.ota` is where firmware OTA images are staged.** The protocol spec records that
   path string, and CCS has a `firmwareUpdateHandler` that very likely acts on what is
   staged there. **Writing anything to that path, or to any path you have not deliberately
   chosen, risks bricking the watch.** Hardcode an allowlist of target paths in your probe
   script and make an unrecognised path a hard error, not a warning.
2. **Never write over a path that holds real data** — activity `.fit` files, `SharedData/`,
   any app's folder. Pick a scratch directory that nothing reads.
3. **Never run this concurrently with USB mass storage.** `una-sdk` documents that USB-MSC
   writes concurrent with BLE sync corrupt the exFAT volume. Unplug USB first, and say so in
   the method section so a reader knows you did.

Before the first write: **pull a full copy of anything you would be sad to lose** — the read
path works, use it. And have the watch's recovery procedure to hand.

If `deleteHandler` / `makeDirHandler` turn out to work (they exist in the firmware strings
but were never exercised), cleaning up after yourself is possible — but do not assume it
until you have demonstrated it.

---

## 1. What this unblocks

| Decision | Where it lives | How it depends on this |
|---|---|---|
| Is transfer phone-first or desktop-only? | `MAP_DELIVERY_WORKFLOW.md` § 2.4, risk R2 | 3 MiB in 1 min is a product; in 20 min it is not |
| What size should catalog packs default to? | same, § 2.1 | The ceiling is a throughput × patience budget |
| Does `rawtiles` need a verifiable prefix? | `RAWTILES_SPEC_UPDATE_PROMPT.md` C6 — **explicitly blocked on this** | If transfers are fast and reliable, resume is unnecessary and the spec change is dead. If slow, chunk size and integrity granularity follow from the measured framing |
| What do we ask UNA for? | the RFC that follows this | "Please document the write path" lands very differently with a measured number attached |

**Do not design any of the above during this investigation.** Measure, then stop.

---

## 2. Settled — do not redo

Prior work on `investigate/ble-large-file-transfer` established all of this against real
hardware. Read `Docs/Investigations/2026-07-29-hardware-config-recovery/BLE-COMPANION-protocol-spec.md`
before starting.

- **The FTS characteristic is `adaf0002-4669-6c65-5472-616e73666572`**, and it multiplexes
  several sub-protocols by leading opcode byte.
- **The read path works, phone-free, from Linux.** `prototype/una_ble_client.py` lists
  directories and pulls `.fit` files that validate by FIT header arithmetic and CRC-16.
- **`readHandler`, `writeHandler`, `listDirHandler` all exist** in firmware strings.
  `writeHandler`'s own log strings are `Invalid offset...` and
  `Writing: %u%% (%u/%u bytes) [%s]`.
- **A phone→watch upload was observed live** — the `0x20`/`0x21`/`0x22` family, Write
  Commands, to `/GPS_EPO/GPS.DAT`. Framing was not decoded; that is your job.
- **There is no 64 KB ceiling.** `offset`, `total` and `chunklen` are genuine 32-bit fields.
  An earlier 16-bit parse produced a false ceiling and was retracted.
- **Use `AcquireNotify`'s raw socket, not D-Bus `PropertiesChanged`** — the latter was found
  to silently coalesce and drop rapid successive notifications. `NotifyStream` already does
  this correctly; do not "simplify" it.

---

## 3. The measurements, in order

Each step is cheap and each one can change what the next step is worth doing. **Do them in
order and write down the number before moving on.**

### M1 — Baseline: what is the read path's actual throughput?

You cannot interpret a write number without a read number beside it. Time `read_file()`
against a file of known size (the `.fit` activities already work). Report bytes/second, the
wall-clock, and the spread over at least 3 runs.

Also capture, from `AcquireNotify`'s reply: **the negotiated MTU**. It is already available
as `NotifyStream.mtu` and is currently unused. Write it down.

### M2 — Is it round-trip-bound or bandwidth-bound?

**This is the central diagnostic and it is the cheapest experiment in the list.**

The read path requests one chunk per round trip at a default `chunk_len = 128`. If
throughput is `chunk_len / RTT`, then raising `chunk_len` raises throughput linearly until
something else binds. If throughput is flat as `chunk_len` rises, you are bandwidth-bound and
the connection parameters are the lever instead.

`read_file()` already takes `chunk_len` as a parameter. Sweep it — 128, 244, 512, 4096 —
and plot throughput against it. Two things to watch for:

- **The current code assumes one notification carries one complete response**
  (`b = await stream.get()`, then slice `b[16:16+real_chunklen]`). A `chunk_len` larger than
  `MTU − 16` must arrive fragmented across notifications, and this code will silently
  truncate rather than fail loudly. **Fix that before trusting any large-chunk number** —
  reassemble until you have `real_chunklen` bytes.
- Find where the firmware stops honouring the request. It honoured 128 exactly in every
  observed transfer; nobody has asked it for more.

### M3 — Do connection parameters move it?

If M2 says round-trip-bound, the interval is the multiplier. BlueZ exposes connection
parameter negotiation; a peripheral may accept or refuse a shorter interval. Try it, record
what the watch actually grants, and re-run M1's timing.

Report the granted interval, not the requested one.

### M4 — Decode the write framing

The goal. `0x20`/`0x21`/`0x22`, observed as Write Commands to a path.

By analogy with the read path (`0x10` opens with a path, `0x12` continues by offset), expect
something like: `0x20` = start-write carrying path and total size, `0x21` = data chunk at an
offset, `0x22` = completion or acknowledgement. **Do not assume that mapping — it is a guess
from opcode adjacency, and the read path already punished one confident guess about framing.**

Work from `writeHandler`'s own log strings, which tell you what it validates:
`Invalid offset...` means it checks offsets and will reject a bad one — which gives you a
cheap, safe probe. Send a deliberately wrong offset and see what comes back. An error
response is information and costs nothing.

If you have Ghidra set up from the prior pass, decompiling `writeHandler` directly is likely
faster than black-box probing, and safer, because you learn the validation rules before you
exercise them.

### M5 — Measure the write path at realistic sizes

Once framing works: time writes at **1 MiB, 3 MiB, 8 MiB, and 29 MiB**, to a scratch path.
Those are the sizes that matter (`MAP_DELIVERY_WORKFLOW.md` § 2.1 — a city pack is ~3 MiB,
a metro region ~29 MiB).

Report per-size throughput, whether it stays flat as the file grows, and **whether it
completes at all**. Watch for: connection drops mid-transfer, throughput degrading with
offset, the watch's own progress logging (`Writing: %u%%`) diverging from what you sent.

### M6 — What happens when it goes wrong?

Only worth doing if M5 succeeded. Three questions, each one a decision input for `rawtiles`
change C6:

- **Can a transfer resume?** Kill the connection at 50 % and try to continue at the last
  offset. Does the firmware accept a mid-file offset on a fresh connection, or does it insist
  on starting over?
- **What does a partial file look like on the device?** Read it back. Is it truncated at the
  last written offset, zero-filled, or absent entirely?
- **Does the watch validate anything?** Or does it accept whatever bytes arrive at whatever
  offsets, in any order?

---

## 4. Premise traps

- **The read path is not the write path.** Everything you know about `0x10`/`0x11` is a
  hypothesis about `0x20`/`0x21`/`0x22`, not evidence. The two are different handlers.
- **`chunk_len = 128` is a client choice, not a firmware limit.** Every observed transfer
  used 128 because the phone app asked for 128. Do not report it as a constraint.
- **One notification ≠ one response** at large chunk sizes. See M2.
- **BlueZ grants what the peripheral allows.** Report negotiated parameters, never requested
  ones — this is the single easiest way to publish a wrong number.
- **A fast desktop-Linux number may not transfer to Android.** The prior investigation
  flagged BlueZ connection fragility as possibly Linux-specific. If your number is good,
  say explicitly that it is a Linux/BlueZ number and that Android is unmeasured.
- **Advertising windows are short.** The prototype README documents connection flakiness —
  budget for retries, and do not mistake a flaky connection for a slow protocol.
- **Do not extend scope into building a companion app.** Measure the channel. Stop.

---

## 5. Deliverables

1. **An investigation bundle** at
   `una-sdk/Docs/Investigations/<date>-ble-write-path/` — hypothesis, method (including
   "USB was unplugged"), raw timing logs, and a verdict per measurement M1–M6.
2. **One headline number, stated plainly:** sustained write throughput in kB/s, with the
   conditions that produced it and the number of runs behind it.
3. **The write framing**, documented to the same standard as the read path in
   `BLE-COMPANION-protocol-spec.md` § 2.2 — byte layout, opcode meanings, what is validated,
   what is still unknown. Fold it into that document rather than starting a new one.
4. **`una_ble_client.py` extended** with a working `write_file()`, the large-chunk
   reassembly fix from M2, and the path allowlist from § 0.1.
5. **A one-paragraph answer to each of the four questions in § 1**, so the decisions those
   block can be made without re-reading the whole bundle.

**What "done" looks like:** someone can read the headline number, look at the four
decisions in § 1, and make all four without asking a follow-up question. And someone else
with a watch and a Linux box can reproduce the number from the method section alone.
