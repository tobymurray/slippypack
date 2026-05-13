//! slippypack — native CLI for building offline `.upack` tile packs.
//!
//! See `PLAN.md` at the repo root. This binary is the workspace skeleton's
//! placeholder; the Phase 1 first slice (per PLAN.md § First slice) replaces
//! `main` with the real `make` subcommand, `--source synthetic`, URL-template
//! fetching, decode + quantise + write pipeline, and SIGINT-handled atomic
//! `.upack.partial` → rename.

fn main() {
    eprintln!(
        "slippypack: workspace skeleton only — see PLAN.md § First slice for what \
         the Phase 1 deliverable replaces this entry point with."
    );
    std::process::exit(2);
}
