# jqx — Architecture

> **jqx** (JSON Query Executor): one job — query JSON from the command line using
> syntax developers already know (JavaScript), with predictable memory, safe
> execution, and errors that actually help.

Status: design draft v0.1 · Language: Rust (stable, `std` only at runtime)

---

## 1. Goals, non-goals, principles

### 1.1 Goals (in priority order)

1. **Easy to use** — zero cheat-sheet for the 99% case (path, project, filter, print).
2. **Safe execution** — a query can never touch the filesystem, network, environment,
   or run forever / eat all RAM unless the user explicitly allows it.
3. **Memory safe** — `#![forbid(unsafe_code)]` in every crate. No exceptions in v1.
4. **No boilerplate, clean, fast** — standard Rust only: no runtime crates, no macros-heavy
   frameworks, hand-written CLI parsing. Speed comes from design (streaming, zero-copy,
   early exit), not from `unsafe` tricks.

### 1.2 Non-goals

- Not a programming language: no user-defined functions, recursion, variables, modules,
  `import`, `eval`, `async`, classes, prototypes, closures as first-class values.
- Not a JSON mutation/transform engine (use a real language for that). Limited
  reshaping (`pick`, `omit`, object/array literals) is allowed because it is query output.
- Not a JavaScript engine. We borrow *syntax and method names*, not semantics wholesale
  (see §5.3 — we deliberately fix JS footguns).

### 1.3 Design principles

- **Borrow the mental model, fix the footguns.** `users.filter(u => u.age > 21)` works as
  expected; `"1" == 1` does not silently coerce.
- **Honest memory contract.** Memory is `O(nesting depth + largest value materialized by
  the query)`, *not* "O(1)". The tool tells you which (`--explain`, §9).
- **Everything is bounded.** Depth, token size, output size, evaluation steps — all have
  defaults and flags.
- **Errors are a feature.** Every error carries a source span (query) or a JSON path +
  byte offset (data), and suggests a fix when one is computable.
- **Layered crates, one-directional dependencies.** Lower layers know nothing of higher ones.

---

## 2. Research: where `jq` hurts (and what jqx does about it)

Compiled from widely-reported jq user pain points and jq's own design (not an
exhaustive audit; each item maps to a concrete jqx decision).

| # | jq pain point | Why it hurts | jqx answer |
|---|---------------|--------------|------------|
| 1 | **Own Turing-complete DSL** (generators, backtracking, `reduce`/`foreach`, path expressions, `limit`, `label/break`) | Steep learning curve for a "filter some JSON" task; `|`, `,`, `//`, `..` precedence surprises | JS-subset syntax; no user functions/recursion; grammar small enough to fit one page (§5) |
| 2 | **Whole document loaded into RAM** by default; `--stream` exists but emits a path/leaf event format that is awkward to use | Large files/logs are slow or OOM; streaming mode is a different language to learn | Streaming is the *default execution model*; the planner picks it automatically and the output is ordinary values (§6) |
| 3 | **Cryptic runtime errors** (`Cannot index array with string "x"`, no location in input or query) | Debugging by bisecting the filter | Errors show query caret, JSON path (`$.items[3].price`), byte offset, nearby keys, did-you-mean (§8) |
| 4 | **Silent `null` propagation** (`.a.b.c` on missing → `null`) | Missing key vs. real `null` are indistinguishable; typos produce plausible empty output | Distinct *absent* (JS `undefined`) vs `null`; `--strict` turns absent access into an error; `-e` exit codes (§5.3) |
| 5 | **Numbers → IEEE-754 double** (historically); large ints/decimals lose precision, formatting of numbers changes | Wrong IDs/amounts silently — dangerous for 64-bit IDs | Lossless number model: keep original decimal text; exact int compare; opt-in float math (§4.3) |
| 6 | **Shell-quoting hell** (`'.items[] \| select(.a == "x")'`, `--arg`, `--argjson`) | Quoting bugs, injection when interpolating shell vars | Quote-free syntax for common ops, `$name` bound with `--var name=value` (typed), never string-interpolated into the program |
| 7 | **Memory-safety history**: C implementation, refcounted manual memory; CVEs of overflow/UAF/stack-exhaustion class have been reported over the years | Security-sensitive when parsing untrusted JSON | Safe Rust only; explicit depth/size limits; fuzzed (§11) |
| 8 | **Language can reach outside the data** (`env`, `$ENV`, `input_filename`, module `import`/`include`, `debug`/`stderr`, `input`/`inputs`) | Queries from untrusted sources (CI snippets, docs, LLM output) have ambient authority | Query language is *pure*: no I/O, env, time, random, modules. Only flags the human types can do I/O (§7) |
| 9 | **Unbounded evaluation**: a query can loop/recurse forever or explode combinatorially (`[range(1e12)]`, `..` abuse) | Hung pipelines | No recursion/unbounded generators; fuel budget on every evaluation step (§7.2) |
| 10 | **Single record at a time, single thread** for NDJSON | Big log files are CPU-bound on one core | Safe data-parallel NDJSON via `std::thread::scope` with ordered output (§10) |
| 11 | **Output surprises**: key order options, unicode escaping, `-r` vs. strings, NaN/Infinity → `null` | Scripts break on formatting | Insertion-order preserved by default, deterministic formatter, documented `-r`, NaN/Infinity cannot exist in input and are errors in arithmetic (§4.3) |
| 12 | **Invalid JSON handling is all-or-nothing** | One bad line kills a 10 GB run | `--on-error=fail|skip|report` for NDJSON; errors carry line/col (§8) |

---

## 3. System overview

```
 ┌────────────┐   bytes    ┌───────────┐  events   ┌──────────────┐  Value   ┌───────────┐  bytes  ┌────────┐
 │ Input src  │──────────▶│ Tokenizer │─────────▶│ Stream Plan  │────────▶│ Evaluator │───────▶│ Emitter│──▶ stdout
 │ file/stdin │  (chunks)  │ (pull)    │ skip/enter│  executor    │ subtree │ (lazy)    │ values │ fmt    │
 └────────────┘            └───────────┘           └──────▲───────┘         └─────▲─────┘        └────────┘
                                                          │ plan                  │ HIR
                                                   ┌──────┴───────────────────────┴─────┐
 query string ───▶ Lexer ─▶ Parser ─▶ AST ─▶ Sema/Lower ─▶ HIR ─▶ Planner ─────────────┘
                                                         (names, arity, types-lite)
```

Two phases:

1. **Compile phase** (once, microseconds): query text → AST → checked HIR → *stream plan*.
2. **Run phase** (per input document / record): tokens → only the parts the plan needs are
   materialized → evaluated → emitted.

The key insight: **a query usually has a *navigational prefix* (cheap, automaton-able)
and a *computational suffix* (needs real values).** The planner splits them.
`items[].tags.filter(t => t.startsWith("a"))` → prefix `items[]` is streamed element by
element; suffix `.tags.filter(...)` runs on one small materialized element at a time.

---

## 4. Data model

### 4.1 Value

```rust
pub enum Value<'a> {
    Absent,                          // JS `undefined`; never serialized
    Null,
    Bool(bool),
    Number(Number<'a>),
    String(Cow<'a, str>),            // borrowed from input buffer when no escapes
    Array(Vec<Value<'a>>),
    Object(Object<'a>),              // insertion-ordered
}
```

- `'a` ties borrowed strings to the input buffer ⇒ **zero-copy** for the common
  escape-free string. Fully safe; the borrow checker enforces buffer lifetime.
- `Object` = `Vec<(Cow<'a, str>, Value<'a>)>` plus a lazily built index (sorted
  `Vec<u32>` of positions) only when an object exceeds a small size (e.g. 16 keys).
  Small objects (the 99% case) use linear scan: cache-friendly, no hashing, no
  `HashMap` DoS surface.
- **Duplicate keys**: default = last wins for queries, all preserved on pass-through
  (`.`); `--strict-json` = error. Documented, tested.
- Sharing: subtree results use `Rc` only where the evaluator fans out (e.g., object
  literal reusing a subvalue). Single-threaded per record ⇒ no atomics. Parallel mode
  (§10) gives each worker its own values; nothing crosses threads except owned output bytes.

### 4.2 Strings

- Input validated as UTF-8 once per chunk boundary-aware (`std::str::from_utf8`);
  invalid UTF-8 is an error with offset (never silently replaced).
- Escapes (`\n`, `\uXXXX` incl. surrogate pairs; lone surrogates = error) decoded into
  `Cow::Owned` only if present.

### 4.3 Numbers (lossless by default)

```rust
pub enum Number<'a> {
    Int(i64),
    Float(f64),                      // finite only
    Raw(Cow<'a, str>),               // original text when it doesn't fit i64 / f64 exactly
}
```

- Parsing keeps the original lexeme; `Int` if it fits `i64` with no fraction/exponent;
  otherwise `Float` **only if** round-trip text is identical, else `Raw`.
- **Printing a number from input prints the original text** (no `1.0` → `1`, no
  `12345678901234567890` → `1.2345678901234567e19`).
- Comparisons: Int–Int exact; Raw–Raw compared as decimals (string algorithm, no
  allocation needed for sign/magnitude compare); mixed Int/Float compared exactly
  (not by lossy cast).
- Arithmetic (`+ - * / %`, `sum`, `avg`): checked `i64` ops; overflow ⇒ promote to
  `f64` **with a warning flag in diagnostics** (or error under `--strict`). `NaN`/`±Infinity`
  results are errors, never emitted as `null`.
- v1 scope: arithmetic is minimal (needed for predicates like `price * qty > 100`).

---

## 5. Query language (the "JS subset")

### 5.1 Grammar (informal EBNF)

```
query      := expr
expr       := ternary
ternary    := or ( '?' expr ':' expr )?
or         := and ( '||' and )*
and        := cmp ( '&&' cmp )*
cmp        := add ( ('=='|'!='|'<'|'<='|'>'|'>=') add )*
add        := mul ( ('+'|'-') mul )*
mul        := unary ( ('*'|'/'|'%') unary )*
unary      := ('!'|'-') unary | postfix
postfix    := primary ( '.' IDENT | '?.' IDENT | '[' index ']' | '?.[' index ']'
                      | '.' IDENT '(' args? ')' | '?.' IDENT '(' args? ')' )*
primary    := IDENT | '$' IDENT | literal | '(' expr ')' | array_lit | object_lit
index      := expr | slice                          // a[0]  a[-1]  a["key"]  a[1:3]
slice      := expr? ':' expr?
args       := arg (',' arg)*
arg        := lambda | expr
lambda     := IDENT '=>' expr | '(' IDENT (',' IDENT)* ')' '=>' expr
array_lit  := '[' (expr (',' expr)*)? ']'
object_lit := '{' (prop (',' prop)*)? '}'
prop       := IDENT (':' expr)? | STRING ':' expr   // {id, name: u.name}
literal    := NUMBER | STRING | 'true' | 'false' | 'null'
```

- **Root input** is implicit: a bare identifier at the start of a query is a field of the
  root (`users[0].name` ≡ `$.users[0].name`). `$` / `.` alone = the whole document.
  This removes the leading-dot noise of jq while staying unambiguous (identifiers that are
  also builtins are resolved root-field-first; see ADR-5).
- **Lambdas appear only as arguments to builtin methods.** They are not values. This single
  rule eliminates closures, recursion, and unbounded computation by construction.
- Optional implicit-parameter sugar (**v1.1, behind a flag until validated**):
  `users.filter(age > 21)` where bare identifiers inside a lambda-less predicate refer to
  the element.

### 5.2 Builtins (v1 — deliberately small)

| Group | Members |
|-------|---------|
| Array | `map` `filter` `find` `findIndex` `some` `every` `includes` `slice` `at` `first` `last` `take(n)` `skip(n)` `flat` `flatMap` `unique` `uniqueBy` `sort` `sortBy` `reverse` `join` `length` `count(pred?)` `groupBy` `countBy` `keyBy` |
| Aggregate | `sum` `avg` `min` `max` `minBy` `maxBy` |
| Object | `keys` `values` `entries` `pick(...)` `omit(...)` `has(k)` `fromEntries` |
| String | `length` `startsWith` `endsWith` `includes` `toLowerCase` `toUpperCase` `trim` `split` `slice` `replace` (literal only) `matches(pattern)` (linear-time engine, §7.4) |
| Type/convert | `typeof` `isNull` `isArray` `isObject` `isString` `isNumber` `toString` `toNumber` |
| Output control | `-r`/`--raw` (flag, not builtin), `.json()` – re-encode as string |

Everything is **pure and total-with-errors**: no builtin performs I/O, reads the clock,
or uses randomness.

### 5.3 Semantics that intentionally differ from JavaScript

| JS footgun | jqx behavior |
|------------|--------------|
| `==` coerces (`"1" == 1`) | `==`/`!=` have **strict (`===`)** semantics. No `===` token needed (accepted as alias for familiarity). |
| `<`, `>` coerce mixed types | Comparing different types is an **error** (`cannot compare string with number at $.a[2].age`), not `false`. |
| `undefined` vs `null` blurred | Missing field → **Absent**. Absent is dropped from `map` results, from objects literals, and prints nothing. `null` is a real value. `--strict`: absent access is an error. `x ?? d` replaces Absent *and* null. |
| `+` concatenates/adds by coercion | `+` is number add, string concat, or array concat — only when both sides have the same type; otherwise error. |
| Float-only numbers | Lossless numbers (§4.3). |
| `sort()` default lexicographic | `sort` orders by a defined total order: `null < bool < number < string < array < object`; numbers numerically. |
| Out-of-range index → `undefined` | Same (Absent) — and negative indices (`a[-1]`) are supported (ergonomic > JS). |

### 5.4 Result model

A query yields **zero or more values** ("a stream of results"), not exactly one:

- Terminal `[]`-style expansion (`items[].id`) and `-s`-less NDJSON input produce a
  sequence; each value is printed on its own line (`-c`) or pretty-printed.
- Methods that return arrays (`filter`, `map`) yield **one array value** by default.
  Use `items.filter(...)[]` (or the CLI flag `--each`) to iterate it as a stream — this
  keeps the JS semantics (`filter` returns an array) while giving jq-like streaming
  ergonomics explicitly.
- **Exit codes** (documented, stable): `0` output produced, `1` no output (`-e`), `2` usage
  error, `3` query compile error, `4` input error, `5` runtime query error,
  `6` resource limit exceeded.

---

## 6. Streaming engine

### 6.1 Honest memory model

> Peak memory = `O(depth) + O(largest chunk the plan must materialize) + O(buffer)`.

Not O(1). The planner classifies a query and `jqx --explain` reports it:

| Class | Example | Peak memory | Early exit |
|-------|---------|-------------|-----------|
| **S0 – pure stream** | `items[].id`, `a.b.c`, `items[2]` | O(depth + selected element) | yes |
| **S1 – per-element** | `items.filter(x => x.ok).map(x => x.id)` *(when consumed with `[]`/`--each`)*, `items.find(...)`, `items.take(10)` | O(depth + one element) | yes (`find`, `take`, `first`) |
| **S2 – bounded buffer** | `items.at(-1)`, `items.slice(-5)`, `count`, `sum`, `min`, `max`, `some`, `every` | O(k) or O(1) accumulators | partly (`some`/`every`) |
| **S3 – materializing** | `sort`, `sortBy`, `groupBy`, `unique`, `reverse`, `keyBy` | O(size of that array) | no |
| **S4 – whole-document** | queries referencing root from inside a lambda (`items.filter(x => x.id === root.limit)`) | O(document) | no |

The CLI can enforce a ceiling: `--max-memory 256MiB` fails with exit code 6 and an error that
names the offending operation (`sortBy at 1:12 needs the whole array; add .take(n) first or raise --max-memory`).

### 6.2 Pull tokenizer

```rust
pub enum Event<'a> {
    StartObject, EndObject, StartArray, EndArray,
    Key(Cow<'a, str>),
    Scalar(Scalar<'a>),
}
pub trait EventSource<'a> {
    fn next(&mut self) -> Result<Option<Event<'a>>, JsonError>;
    fn skip_value(&mut self) -> Result<(), JsonError>;   // fast-forward without allocating
    fn offset(&self) -> u64;
}
```

- **Chunked reader**: `BufRead`-style ring of 64 KiB (tunable); tokens spanning chunk
  boundaries are handled by a small carry buffer (grows only for a single huge token, capped
  by `--max-token`, default 16 MiB).
- **State machine** hand-written (table-driven for whitespace/structure bytes). No recursion:
  an explicit `Vec<Frame>` stack with a hard `--max-depth` (default **512**) ⇒ **no stack overflow
  on `[[[[...`** — a classic crash/DoS in parsers.
- **`skip_value`**: scans bytes tracking only string/escape state and depth. No UTF-8
  validation of skipped strings by default? **No** — validation still runs (safety over a
  few % speed); `--unchecked-skip` (still safe Rust, just skips validating skipped regions'
  UTF-8) is an explicit opt-in for trusted inputs.
- Speed without `unsafe`: use iterator-based byte scans (`position`, `chunks_exact`, branchless
  classification tables) that LLVM auto-vectorizes. If a future profile proves std-only is
  insufficient, `core::arch` SIMD would go in a **separate, opt-in feature crate** (see ADR-3) —
  never in the default build.

### 6.3 Stream plan executor

The planner lowers the navigational prefix to a tiny **path automaton**:

```rust
enum Step {
    Field(Box<str>),            // .name
    Index(u64),                 // [3]   — counts siblings, skips others
    AnyIndex,                   // []    — each element: enter, hand off, continue
    Slice { start: u64, end: Option<u64> },
    LastN(u32),                 // ring buffer of the final n elements
}
struct StreamPlan { prefix: Vec<Step>, suffix: HirExpr, demand: Demand }
```

Executor loop (conceptually):

1. Tokenizer gives events; executor tracks current JSON path as a stack of `(container, key/idx)`.
2. If the current position **cannot** extend any prefix match → `skip_value()` (no allocation).
3. If it **matches the end of the prefix** → *materialize that subtree* into a `Value`,
   run the suffix evaluator, emit results, drop the subtree (arena/`Vec` freed).
4. If `demand` is satisfied (e.g., `first`, `find` matched, `take(n)` reached,
   `[0]` served) → **stop reading** and close input. Upstream writers get EOF/`SIGPIPE`
   quietly (a closed pipe is *not* an error).

`Demand` flows backward from the sink: `Unbounded | AtMost(n) | First`. Operators like
`find`, `first`, `take`, `some` set it. This gives jq's `limit`/`first` benefits with none of
the syntax.

### 6.4 Input modes

| Mode | Flag | Behavior |
|------|------|----------|
| Single document | default | One top-level value (trailing whitespace only; trailing garbage = error) |
| NDJSON / concatenated | `--ndjson` (autodetected when ≥2 top-level values) | Evaluate per record; supports parallelism (§10) |
| Slurp | `-s/--slurp` | Collect all records into an array (class S3; subject to `--max-memory`) |
| Null input | `-n` | No input; evaluates literals (rare) |

---

## 7. Safety model

### 7.1 Threat model

Assume **both** the JSON *and* the query may be hostile (e.g., a CI snippet, a copy-pasted
StackOverflow answer, or LLM-generated query run against customer data). Goals: no
code execution, no data exfiltration, no crash, no unbounded CPU/RAM/time.

### 7.2 Mechanisms

| Layer | Mechanism |
|-------|-----------|
| Language | Pure, total-with-errors; no I/O, env, time, randomness, modules, `eval`; lambdas are not first-class; no recursion |
| Memory safety | `#![forbid(unsafe_code)]` in all crates; CI greps for `unsafe` and fails; no FFI; no dependencies to audit |
| Stack safety | No recursive-descent on *data* (explicit stack, `--max-depth`); parser recursion on *query* bounded (`MAX_QUERY_DEPTH = 128`) |
| CPU | **Fuel**: each HIR node evaluation decrements a budget (`--fuel`, default 10⁹ steps/record; `0` = unlimited). Exhaustion = exit 6 with the query span that consumed it |
| Memory | Allocation accounting wrapper (`Budget`) charged when materializing arrays/objects/strings; `--max-memory` enforced *before* allocating (checked-arithmetic sizes, `try_reserve`) so OOM aborts never happen |
| Output | `--max-output` (default unlimited for stdout streaming, bounded when `--capture`); cartesian blow-ups impossible (no unbounded generators), array literals/`flatMap` charged to memory budget |
| Integer | `checked_*` everywhere; casts via `try_from`; `clippy::arithmetic_side_effects` enabled at workspace level |
| Time | `--timeout 30s` (wall clock, checked at record/chunk boundaries and every N fuel steps) |
| I/O authority | Only CLI flags touch files: input paths, `--output`. The *query* never names a path. Writing is limited to stdout/`--output` |
| Variables | `--var name=value` (string) / `--var-json name=<json>` bind `$name` as **data**; never parsed as query text ⇒ no injection |

### 7.3 Defaults vs. flags

Safe by default; relaxations are explicit and loud: `--max-depth`, `--max-memory`,
`--fuel`, `--timeout`, `--max-token`, `--unchecked-skip`.

### 7.4 Regex

jq's `test`/`match` use Oniguruma (external C). jqx v1: `startsWith/endsWith/includes`
cover most needs; `matches(pattern)` uses a **built-in Thompson-NFA / Pike-VM** (linear time,
no backtracking, no catastrophic patterns, no look-around/back-references). Pattern size and
steps charged to fuel. Deferred to milestone M6 if schedule demands.

---

## 8. Diagnostics

Three classes, one shared renderer (`jqx-diag`):

**Query compile errors** — span + caret + suggestion:

```
error[Q0012]: unknown method `fliter`
 --> query:1:7
  |
1 | users.fliter(u => u.age > 21)
  |       ^^^^^^ did you mean `filter`?
```

**Query runtime errors** — span + data location + context:

```
error[R0031]: cannot compare number with string
 --> query:1:26      users.filter(u => u.age > "21")
                                       ^^^^^^^^^^^^
  at $.users[3].age  (input.json:812:17, byte 20412)
  left : 30   (number)
  right: "21" (string)
  hint : compare like with like; use toNumber("21") if the string is numeric
```

**Input errors** — JSON syntax with line/col, byte offset, and a code frame; NDJSON adds
record number; with `--on-error=skip` bad records are counted and summarized to stderr.

Rules:
- Missing-key errors (`--strict`) list available keys (truncated) and nearest match by
  edit distance.
- Stable error codes (`Q####`, `R####`, `J####`) documented in `docs/errors.md`.
- Color only if stderr is a TTY and `NO_COLOR` unset; `--color=always|never|auto`.
- `--error-format=json` for tooling.

---

## 9. CLI design (hand-written, no `clap`)

```
jqx [OPTIONS] <QUERY> [FILE...]        # '-' or no FILE = stdin
```

| Flag | Meaning |
|------|---------|
| `-r, --raw` | Print top-level strings without quotes |
| `-c, --compact` / `-p, --pretty` | Output style (default: pretty on TTY, compact when piped) |
| `-0, --null-sep` / `--tab` / `--indent N` | Output separators / formatting |
| `-e, --exit-status` | Exit 1 if no output / last value is `null`/Absent |
| `-s, --slurp`, `-n, --null-input`, `--ndjson` | Input modes (§6.4) |
| `--each` | Iterate a result array as separate outputs |
| `--strict` | Absent access, overflow promotion, duplicate keys become errors |
| `--var k=v`, `--var-json k=JSON` | Bind `$k` (data, not code) |
| `--explain` | Print the plan: streamability class, early-exit, estimated memory, per-stage notes |
| `--max-depth/--max-memory/--fuel/--timeout/--max-token` | Resource limits (§7) |
| `--jobs N` | Parallel NDJSON workers (default = available cores for files > 1 MiB) |
| `--on-error fail\|skip\|report` | NDJSON bad-record policy |
| `-h/--help`, `-V/--version`, `--completions SHELL` | Meta |

`--explain` example:

```
$ jqx --explain 'items.filter(i => i.ok).map(i => i.id)' big.json
plan:   stream  items[]  →  filter  →  map      class S1 (per-element)
memory: O(depth + 1 element) ≈ depth × 64 B + element size
exit:   reads whole input (no early exit available)
note:   .filter/.map yield an array; add `[]` or --each to stream results incrementally
```

Argument parsing: ~200 lines of `std::env::args_os` code in `jqx-cli`, table-driven,
`OsString`-safe (non-UTF-8 paths supported), unit-tested for ambiguity.

---

## 10. Performance strategy (std-only, safe)

| Technique | Where |
|-----------|-------|
| **Skip-don't-parse**: `skip_value` on non-matching subtrees; no allocation | tokenizer / executor |
| **Zero-copy strings** via `Cow<'a, str>` | `Value` |
| **Early exit** from `Demand` propagation | planner/executor |
| **Per-record reuse**: reusable `Vec`s / scratch buffers cleared, not reallocated | evaluator |
| **Small-object linear scan**; lazy index for big objects | `Object` |
| **Compile once**: lambdas lowered to HIR with resolved slots (`u32` variable indices) — no name lookup at run time | sema |
| **Buffered output**: `BufWriter` 64 KiB; number/string escape fast paths; direct byte writes, no `format!` in hot path | emitter |
| **Parallel NDJSON**: reader thread splits on `\n` into batches (~1 MiB); N workers (`std::thread::scope`, `mpsc` or `Mutex<VecDeque>`) evaluate batches; a sequence-numbered reorder buffer emits output **in input order**. No shared mutable `Value`s | `jqx-cli` driver |
| **mmap?** Not in v1 (needs `unsafe`/crate). Revisit as opt-in feature crate | — |

Targets to **validate by benchmarking** (not promises): startup < 5 ms; skip throughput
≥ 1 GB/s/core on typical JSON; full-materialize ≥ 300 MB/s/core; `items[].id` on 1 GB
NDJSON saturates disk/page cache with `--jobs`.

---

## 11. Workspace & crate layout

```
jqx/
├─ Cargo.toml                  # [workspace], lints, forbid(unsafe_code) via [workspace.lints]
├─ ARCHITECTURE.md
├─ docs/{language.md, errors.md, adr/}
├─ crates/
│  ├─ jqx-json      # Value, Number, Object, tokenizer (EventSource), serializer. No query knowledge.
│  ├─ jqx-diag      # Span, Diagnostic, renderer, edit-distance suggestions
│  ├─ jqx-syntax    # Lexer, Parser, AST (spans on every node). Depends: diag
│  ├─ jqx-hir       # Name resolution, arity/type-lite checks, lambda slot allocation, constant folding. Depends: syntax, diag
│  ├─ jqx-plan      # Stream planner: prefix automaton, Demand, memory classification, --explain. Depends: hir
│  ├─ jqx-eval      # Evaluator over Value + Budget/fuel, builtin library. Depends: json, hir, diag
│  ├─ jqx-engine    # Orchestrator: wires tokenizer ↔ plan executor ↔ evaluator; public library API
│  ├─ jqx-regex     # (M6) linear-time regex. Standalone
│  └─ jqx-cli       # Arg parsing, I/O, TTY/color, threading, exit codes. Binary `jqx`
└─ tests/ fuzz/ benches/
```

Dependency rule: arrows only point downward; `jqx-json` and `jqx-diag` are leaves.
**Public library API** (`jqx-engine`) so jqx is embeddable:

```rust
let q = jqx::Query::compile("users.filter(u => u.age > 21).map(u => u.name)")?;
let mut out = q.run_reader(std::io::stdin().lock(), Limits::default())?;   // impl Iterator<Item = Result<Value>>
for v in out { println!("{}", v?); }
```

Boilerplate policy: no proc-macros, no derive crates; errors are plain enums with
hand-written `Display` + `From` (a ~30-line pattern shared via a tiny internal macro_rules
at most).

---

## 12. Testing & quality

| Layer | Strategy |
|-------|----------|
| JSON conformance | Run the public **JSONTestSuite** (`y_*` must accept, `n_*` must reject, `i_*` documented per policy) from `tests/data/` |
| Parser/lexer | Golden tests: query → AST dump; error cases → rendered diagnostics (snapshot files, hand-rolled snapshot helper) |
| Semantics | Table tests per builtin including edge cases (empty arrays, Absent vs null, huge ints, `-0`, surrogate pairs) |
| Streaming equivalence | **Property test:** for random JSON + random queries, `eval(stream plan)` ≡ `eval(DOM, whole doc)`. The DOM evaluator is the reference oracle; the planner may never change results |
| Differential | A curated corpus translated jq ↔ jqx on the 99% subset, compared against installed `jq` (dev-only, skipped if absent) |
| Fuzzing | Tokenizer, parser, and end-to-end (query + data) fuzz targets (dev-tool only; runtime remains dep-free). Assertion: never panic, never exceed budgets |
| Chunk-boundary tests | Same input fed 1 byte at a time vs whole must yield identical events |
| Resource tests | Deep nesting, giant tokens, billion-laughs-style query/data shapes hit limits cleanly (exit 6), never abort/OOM |
| Benchmarks | `benches/` with a std `Instant` harness; tracked in CI against regressions (>10%) |
| CI | `cargo fmt --check`, `clippy -D warnings` (+ `pedantic` selected), `unsafe` grep, MSRV build, Miri on `jqx-json`/`jqx-eval` tests (defense in depth even without `unsafe`) |

---

## 13. Roadmap

| Milestone | Deliverable | Exit criteria |
|-----------|-------------|---------------|
| **M0** | `jqx-json`: tokenizer + `Value` + serializer + number model | JSONTestSuite pass; chunk-boundary & fuzz clean |
| **M1** | `jqx-syntax` + `jqx-diag`: lexer, parser, rich compile errors | Grammar in §5.1 fully parsed; golden diagnostics |
| **M2** | `jqx-hir` + `jqx-eval` (DOM mode): all v1 builtins, fuel/memory budgets | Reference oracle complete; semantics test tables green |
| **M3** | `jqx-plan` + engine: stream plan, skip, early exit, `--explain` | Property test: stream ≡ DOM on 1M random cases |
| **M4** | `jqx-cli`: flags, exit codes, color, NDJSON modes | Usable `jqx` binary; man page; shell completions |
| **M5** | Parallel NDJSON, perf tuning, benchmarks published | Targets in §10 met or revised with data |
| **M6** | Regex engine, implicit-param sugar (if validated), stabilize 1.0 language spec | Language frozen; semver for CLI + library |

---

## 14. Architecture decision records (summary)

| ADR | Decision | Rationale / trade-off |
|-----|----------|-----------------------|
| 1 | **Own JSON engine, no `serde_json`** | Needed for streaming events, lossless numbers, borrowed strings, and depth limits. Cost: we own correctness → mitigated by JSONTestSuite + fuzzing |
| 2 | **Streaming-by-default with a planner**, DOM evaluator as oracle | Achieves large-file ergonomics without a second "stream language"; oracle makes streaming bugs detectable |
| 3 | **`forbid(unsafe_code)` everywhere; SIMD only as an opt-in separate crate** | Memory safety is a headline goal; performance achieved algorithmically first |
| 4 | **Lambdas only as builtin-method args** | Removes closures/recursion ⇒ termination, simple planner, simple mental model |
| 5 | **Bare identifier = root field**; reserved builtin names can't be bare roots (use `$.length` or `["length"]`) | Reduces noise vs. `.foo`; avoids ambiguity by a tiny, documented reserved-word list |
| 6 | **Strict `==`, type-checked comparisons, Absent ≠ null** | Fixes the top JS and jq footguns while keeping syntax familiar |
| 7 | **Lossless numbers** | Correct IDs/amounts matter more than a 1-word enum |
| 8 | **Fuel + memory budgets on by default** | Safe execution requirement; cost is a counter decrement per node |
| 9 | **Hand-written CLI parser, no `clap`** | "Zero dependency" principle; small surface; faster startup/compile |
| 10 | **Honest memory classes (S0–S4) surfaced to the user** | Avoids the myth of "O(1) RAM"; users can reason and cap |

---

## 15. Open questions (need a decision before M1)

1. **Implicit-parameter sugar** (`filter(age > 21)`) in 1.0 or later? It's the biggest ergonomic win
   but complicates scoping rules and error messages.
2. **Result model for `filter/map`**: array-by-default (JS-faithful, current proposal) vs.
   stream-by-default (jq-faithful). The current proposal favors JS expectations plus `[]`/`--each`.
3. **Output of Absent in arrays/objects**: drop silently (proposed) vs. error under `--strict` only.
4. **Duplicate-key policy** default: last-wins (proposed) vs. error.
5. **Include YAML/TOML/CSV input later?** (Proposed: out of scope for 1.0; keep `EventSource`
   trait-based so other formats can plug in without touching the planner.)
6. **Windows/Unicode console** specifics (UTF-8 output, color) — owned by `jqx-cli`; test matrix needed.
