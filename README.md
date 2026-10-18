# jqx

<p align="center">
  <strong>You know JavaScript? You already know jqx.</strong><br>
  The post-modern JSON query executor for developers who want intuitive, zero-fuss terminal queries.
</p>

<p align="center">
  <a href="https://github.com/sapirrior/jqx/actions"><img src="https://img.shields.io/github/actions/workflow/status/sapirrior/jqx/release.yml?branch=main&style=flat-square" alt="Build Status"></a>
  <a href="https://github.com/sapirrior/jqx/releases/latest"><img src="https://img.shields.io/github/v/release/sapirrior/jqx?style=flat-square" alt="Latest Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat-square" alt="License"></a>
</p>

---

## Why jqx?

Every developer inspects JSON in the terminal — debugging APIs, parsing production logs, or filtering CI outputs.

Yet traditional tools force you to wrestle with arcane symbols and cryptic syntax that you have to look up on StackOverflow every single time.

**Why learn a whole new language just to filter JSON?**

With **`jqx`**, your existing JavaScript mental model just works:
- **`filter(...)`** and **`map(...)`** straight from your muscle memory.
- **`?.`** optional chaining that safely traverses without crashing on missing keys.
- **`find()`**, **`sort()`**, **`length()`**, **`unique()`**, and arrow functions that feel like second nature.

```bash
# Old way: What was the filter syntax again?
curl -s https://api.github.com/repos/rust-lang/rust/pulls | jq '.[] | select(.user.login == "bors") | .title'

# The jqx way: Clean, natural, and memorable
curl -s https://api.github.com/repos/rust-lang/rust/pulls | jqx 'filter(.user.login == "bors") | map(.title)'
```

---

## Instant Install

Install the standalone, zero-dependency binary in seconds:

```bash
curl -fsSL https://raw.githubusercontent.com/sapirrior/jqx/main/install.sh | bash
```

Or install from source with Cargo:

```bash
cargo install --git https://github.com/sapirrior/jqx.git
```

> Prebuilt standalone binaries are also available for Linux (x64, arm64), macOS (Intel, Apple Silicon), Windows, and Android (Termux) on the [Releases page](https://github.com/sapirrior/jqx/releases).

---

## The 30-Second Tour

### 1. Navigating Fields & Optional Chaining
Access properties directly with `.` or use `?.` for safe navigation that never crashes on missing keys:

```bash
# Safe traversal: returns null if profile or email is missing
cat user.json | jqx '.user?.profile?.email'
```

### 2. Filtering Collections
Filter lists with intuitive comparison operators (`==`, `!=`, `>`, `<`, `&&`, `||`):

```bash
# Find active premium users over age 25
cat users.json | jqx 'filter(.age >= 25 && .is_premium)'
```

### 3. Transforming & Reshaping
Chain standard array methods like `map()`, `sort()`, and `flatten()`:

```bash
# Extract names, sort alphabetically, and pick the first 5
cat users.json | jqx 'map(.name) | sort() | slice(0, 5)'
```

### 4. Custom Projections
Shape the output into exactly the format you need:

```bash
# Construct a clean summary object
cat telemetry.json | jqx 'map({ host: .hostname, load: .metrics.cpu_usage })'
```

---

## Everyday Recipes

Here are common tasks made effortless:

| Task | Command |
| :--- | :--- |
| **Get array length** | `jqx 'length()'` |
| **Pick first or last** | `jqx 'first()'` or `jqx 'last()'` |
| **Find single item** | `jqx 'find(.id == "req_981")'` |
| **Extract object keys** | `jqx 'keys()'` |
| **Deduplicate items** | `jqx 'unique()'` |
| **Sort by field** | `jqx 'sortBy(.created_at)'` |
| **Sum a numeric field** | `jqx 'map(.amount) | sum()'` |
| **Average value** | `jqx 'map(.latency_ms) | avg()'` |
| **Compact JSON output** | `jqx -c 'filter(.status == 500)'` |
| **Extract raw string** | `jqx -r '.token'` |

---

## Helpful Diagnostics

No more mysterious syntax errors. When something goes wrong, `jqx` pinpoints the exact token and suggests fixes:

```
Error: Unknown identifier 'filtr'
  --> query:1:1
   |
 1 | filtr(.active)
   | ^^^^^ did you mean 'filter'?
```

---

## Built for Speed and Reliability

- **Single Static Binary**: Download and run anywhere — no Node.js, Python, or runtime libraries required.
- **Instant Cold Startup**: Starts in sub-millisecond time.
- **100% Memory Safe**: Written entirely in Rust with zero unsafe code (`#![forbid(unsafe_code)]`).
- **Standard Compliant**: Full UTF-8 support and RFC 8259 JSON compliance.

---

## Command Line Options

```text
Usage: jqx [OPTIONS] [QUERY] [FILE]

Arguments:
  [QUERY]  Filter/transformation expression (defaults to '.')
  [FILE]   Input JSON file (reads from stdin if omitted)

Options:
  -c, --compact        Emit compact single-line JSON instead of pretty-printed
  -r, --raw-output     Emit raw string output without quotes
  -s, --slurp          Read entire input stream into an array of values
  -n, --null-input     Do not read any input; evaluate query with null
  -C, --color <WHEN>   Control colorized output: auto, always, never [default: auto]
  -h, --help           Print help information
  -V, --version        Print version information
```

---

## Contributing

Contributions, feedback, and ideas are warmly welcome! Feel free to open an issue or submit a pull request on [GitHub](https://github.com/sapirrior/jqx).

---

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](http://www.apache.org/licenses/LICENSE-2.0))
- MIT License ([LICENSE-MIT](http://opensource.org/licenses/MIT))

at your option.
