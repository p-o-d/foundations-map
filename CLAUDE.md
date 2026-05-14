# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`foundations-map` — Rust binary crate (edition 2024), currently in early bootstrapping stage.

## Commands

```bash
cargo build          # compile
cargo run            # build + run
cargo test           # run all tests
cargo test <name>    # run single test by name substring
cargo clippy         # lint
cargo fmt            # format
```

## Structure

Single binary crate, entry point `src/main.rs`. No library crate split yet.
