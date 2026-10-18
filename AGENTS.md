# Guidelines & Rules for Agents (AGENTS.md)

This file defines the mandatory development principles, boundaries, and rules of engagement for all AI agents working on the `jqx` project.

---

## 1. Do Only What Is Explicitly Requested
- **Strict Scope Control**: Do only what is explicitly instructed by the user. Do not jump ahead, do not implement unrequested features, and do not make unsolicited architectural changes.
- **No Speculative Coding**: Never create files, scaffold abstractions, or add "nice-to-have" helpers unless specifically asked.
- **Discuss Before Major Actions**: When facing architectural choices or ambiguities, discuss and confirm with the user first.

---

## 2. Testing Philosophy: Critical & High-Value Only
- **Zero Bloat / No Over-engineering**: Do NOT write voluminous, trivial, or redundant tests for every minor helper or internal function.
- **Critical Path Only**: Write tests *only* for:
  - Fundamental correctness boundaries (e.g., standard compliance edge cases).
  - High-risk invariants (e.g., buffer boundaries, recursion/nesting limits, fuel exhaustion).
  - Equivalence oracle verification (e.g., streaming plan evaluation matching DOM evaluation).
- **Quality over Quantity**: A handful of focused, high-leverage tests that rigorously stress invariants is vastly superior to dozens of superficial tests.

---

## 3. Engineering & Code Standards
- **Standard Library Only (`std`)**: No third-party runtime crates, no macro-heavy frameworks, no external dependencies.
- **Memory Safety Guaranteed**: `#![forbid(unsafe_code)]` across all crates without exception.
- **Clean and Idiomatic**: Keep code minimal, readable, and direct. Avoid unnecessary trait layers, complex macro abstractions, or premature indirection.
- **Layered Crate Boundaries**: Respect the crate hierarchy outlined in `jqx.md`. Lower-level crates must never know about or depend on higher-level crates.
- **Purposeful Documentation**:
  - Every source file must begin with a concise, useful module-level doc comment (`//!`) explaining specifically what the file does and its role in the system.
  - No random, noisy, or trivial inline comments. Comments must be informative, explaining non-obvious invariants, design choices, or domain logic.
  - Preserve all existing comments, specifications, and docstrings unless explicitly instructed otherwise.
