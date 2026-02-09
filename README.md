<div align="center">

![Littrs Logo](https://github.com/chonkie-inc/littrs/blob/main/assets/littrs.png?raw=true)

# Littrs

### Keep your LLM's 💩 code where it belongs — in a sandbox.

[![Crates.io](https://img.shields.io/crates/v/littrs.svg)](https://crates.io/crates/littrs)
[![PyPI version](https://img.shields.io/pypi/v/littrs.svg)](https://pypi.org/project/littrs/)
[![License](https://img.shields.io/github/license/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/blob/main/LICENSE)
[![CI](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml/badge.svg)](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml)
[![GitHub stars](https://img.shields.io/github/stars/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/stargazers)

</div>

---

LLMs are better at writing Python than crafting JSON tool calls. But running LLM-generated code means either spinning up containers, paying for sandboxing services, or gambling with `exec()`. Littrs takes a different approach: a Python sandbox that embeds directly into your Rust or Python application as a library. No containers, no network calls, no infrastructure — just `pip install` or `cargo add` and go.

You register functions as tools, hand the sandbox some LLM-generated code, and get back a result. The sandbox compiles Python to bytecode and runs it on a stack-based VM with zero ambient capabilities — no filesystem, no network, no env vars. The only way sandboxed code can reach the outside world is through tools you explicitly provide.

* **Tool registration** — `@sandbox.tool` in Python, `#[tool]` in Rust. Inject variables with `sandbox["x"] = val`, run code with `sandbox(code)`
* **Resource limits** — cap bytecode instructions and recursion depth per call, enforced at the VM level and uncatchable by `try`/`except`
* **Stdout capture** — `print()` output collected and returned separately from the result
* **Auto-generated tool docs** — `describe()` produces Python-style signatures and docstrings, ready to paste into a system prompt
* **Built-in modules** — `json`, `math`, and `typing` available out of the box with `Sandbox(builtins=True)` / `Sandbox::with_builtins()`. Register custom modules with `.module()`
* **File mounting** — mount host files into the sandbox with read-only or read-write access. Sandbox code uses `open()` to read/write; writes persist back to the host. `sandbox.files()` lets you inspect current writable file contents
* **WASM isolation** — optional stronger sandboxing via an embedded wasmtime guest module with memory and fuel limits
* **Fast startup** — no interpreter boot, no runtime to load. Create a sandbox, register tools, run code

Littrs implements enough Python for an LLM to call tools, process results, handle errors, and return values. It does not support third-party packages, classes, closures, `async`/`await`, `finally`, or `match` — see the [ROADMAP](ROADMAP.md) for what's planned and the full list of [supported features](FEATURES.md).

## Installation

### Rust

```bash
cargo add littrs
```

### Python

```bash
uv pip install littrs
```

## Usage

Littrs can be called from Rust or Python. See the [ROADMAP](ROADMAP.md) for planned features.

### Python

```python
from littrs import Sandbox

sandbox = Sandbox()

@sandbox.tool
def get_weather(city: str, units: str = "celsius") -> dict:
    """Get current weather for a city."""
    return {"city": city, "temp": 22, "units": units}

result = sandbox("get_weather('London')")
# result == {"city": "London", "temp": 22, "units": "celsius"}
```

The `@sandbox.tool` decorator registers your function with its full signature — the LLM code calls it like a normal Python function. The sandbox is also callable: `sandbox(code)` is shorthand for `sandbox.run(code)`.

Variables persist across calls, and you can inject values directly:

```python
sandbox["user_id"] = 42
sandbox("name = get_weather('London')['city']")
sandbox("name")  # "London"
```

#### Resource Limits

```python
sandbox.limit(max_instructions=10_000, max_recursion_depth=50)

try:
    sandbox.run("while True: pass")
except RuntimeError as e:
    print(e)  # "Instruction limit exceeded (limit: 10000)"
```

Resource limit errors are **uncatchable** — `try`/`except` in the sandbox code cannot suppress them.

#### Capturing Print Output

`capture()` returns both the result and everything that was `print()`-ed:

```python
result, printed = sandbox.capture("""
for i in range(5):
    print(i)
"done"
""")
# result  == "done"
# printed == ["0", "1", "2", "3", "4"]
```

#### Tool Documentation for LLM Prompts

`describe()` auto-generates Python-style signatures and docstrings from registered tools, ready to embed in a system prompt:

```python
print(sandbox.describe())
# def get_weather(city: str, units: str = 'celsius') -> dict:
#     """Get current weather for a city."""
```

#### Low-level Registration

If you need to bypass the decorator (e.g. registering a function that takes raw positional args):

```python
def fetch_data(args):
    return {"id": args[0], "name": "Example"}

sandbox.register("fetch_data", fetch_data)
```

#### Imports & Built-in Modules

Create a sandbox with `builtins=True` to enable `json`, `math`, and `typing` modules:

```python
sandbox = Sandbox(builtins=True)

result = sandbox("""
import json
data = json.loads('{"name": "Alice", "score": 95}')
data["score"]
""")
# result == 95
```

`from ... import` works too:

```python
sandbox("""
from math import sqrt, pi
sqrt(pi)
""")
```

Register custom modules with `.module()`:

```python
sandbox.module("config", {"version": "1.0", "debug": False})
sandbox("import config; config.version")  # "1.0"
```

#### File Mounting

Mount host files into the sandbox so LLM-generated code can read input and write output without full filesystem access:

```python
sandbox.mount("data.json", "./data/input.json")                    # read-only (default)
sandbox.mount("output.txt", "./output/result.txt", writable=True)  # read-write

result = sandbox("""
f = open("data.json")
data = f.read()
f.close()

f = open("output.txt", "w")
f.write("processed: " + data)
f.close()
""")

# Inspect written files from the host
sandbox.files()  # {"output.txt": "processed: ..."}
```

Unmounted paths raise `FileNotFoundError`; writing to read-only mounts raises `PermissionError`. Both are catchable with `try`/`except` inside the sandbox.

#### WASM Sandbox (Stronger Isolation)

For stronger isolation, Littrs can run the interpreter inside a WebAssembly guest module with memory isolation and fuel-based computation limits:

```python
from littrs import WasmSandbox, WasmSandboxConfig

config = WasmSandboxConfig().with_fuel(1_000_000).with_max_memory(32 * 1024 * 1024)
sandbox = WasmSandbox(config)

result = sandbox.run("sum(range(100))")
assert result == 4950
```

### Rust

The `#[tool]` macro is the easiest way to register tools. Write a normal function with doc comments, and the macro generates everything needed for registration and LLM documentation:

```rust
use littrs::Sandbox;
use littrs_macros::tool;

/// Get current weather for a city.
///
/// Args:
///     city: The city name
///     units: Temperature units (C or F)
#[tool]
fn get_weather(city: String, units: Option<String>) -> String {
    format!("{}: 22°{}", city, units.unwrap_or("C".into()))
}

let mut sandbox = Sandbox::new();
sandbox.add(get_weather::Tool);

let result = sandbox.run(r#"get_weather("London")"#).unwrap();
```

The `#[tool]` macro handles type conversion from `PyValue` automatically. `sandbox.add()` registers the tool with its full metadata.

Variables persist across `run()` calls:

```rust
sandbox.run("x = 10").unwrap();
sandbox.run("y = 20").unwrap();
let result = sandbox.run("x + y").unwrap();
assert_eq!(result, PyValue::Int(30));
```

#### Resource Limits

```rust
use littrs::{Sandbox, Limits};

let mut sandbox = Sandbox::new();
sandbox.limit(Limits {
    max_instructions: Some(10_000),
    max_recursion_depth: Some(50),
});

let err = sandbox.run("while True: pass").unwrap_err();
assert!(err.to_string().contains("Instruction limit"));
```

Resource limit errors are **uncatchable** — `try`/`except` in the sandbox code cannot suppress them. This is by design: the host must always be able to regain control.

#### Tool Documentation

`describe()` auto-generates Python-style docs for all registered tools, suitable for embedding in an LLM system prompt:

```rust
let docs = sandbox.describe();
// def get_weather(city: str, units?: str) -> str:
//     """Get current weather for a city."""
```

#### Capturing Print Output

```rust
let mut sandbox = Sandbox::new();
let output = sandbox.capture(r#"
for i in range(5):
    print(i)
"done"
"#).unwrap();

assert_eq!(output.output, vec!["0", "1", "2", "3", "4"]);
assert_eq!(output.value, PyValue::Str("done".to_string()));
```

#### Low-level Registration

For cases where the `#[tool]` macro isn't suitable, you can register closures directly:

```rust
use littrs::{Sandbox, PyValue};

let mut sandbox = Sandbox::new();

sandbox.register_fn("fetch_data", |args| {
    let id = args[0].as_int().unwrap_or(0);
    PyValue::Dict(vec![
        (PyValue::Str("id".to_string()), PyValue::Int(id)),
        (PyValue::Str("name".to_string()), PyValue::Str("Example".to_string())),
    ])
});
```

#### Imports & Built-in Modules

Use `Sandbox::with_builtins()` to enable `json`, `math`, and `typing` modules:

```rust
use littrs::{Sandbox, PyValue};

let mut sandbox = Sandbox::with_builtins();

let result = sandbox.run(r#"
import json
data = json.loads('{"name": "Alice", "score": 95}')
data["score"]
"#).unwrap();
assert_eq!(result, PyValue::Int(95));
```

Register custom modules with `.module()`:

```rust
use littrs::{Sandbox, PyValue};

let mut sandbox = Sandbox::new();
sandbox.module("config", |m| {
    m.constant("version", PyValue::Str("1.0".into()));
    m.function("get_flag", |_args| PyValue::Bool(true));
});

let result = sandbox.run("import config; config.version").unwrap();
assert_eq!(result, PyValue::Str("1.0".into()));
```

#### File Mounting

Mount host files into the sandbox for controlled file I/O:

```rust
use littrs::{Sandbox, PyValue};

let mut sandbox = Sandbox::new();
sandbox.mount("data.json", "./data/input.json", false);      // read-only
sandbox.mount("output.txt", "./output/result.txt", true);     // read-write

sandbox.run(r#"
f = open("data.json")
content = f.read()
f.close()

f = open("output.txt", "w")
f.write("processed")
f.close()
"#).unwrap();

let files = sandbox.files();  // {"output.txt": "processed"}
```

Unmounted paths raise `FileNotFoundError`; writing to read-only mounts raises `PermissionError`. Both are catchable inside the sandbox with `try`/`except`.

## Alternatives

Littrs is designed for one specific use case: **running code written by AI agents safely and cheaply**. It trades language completeness for simplicity, speed, embeddability, and zero infrastructure requirements.

| Tech | Security | Start latency | Embeddable | Resource limits | Tool registration | WASM isolation | Setup |
|---|---|---|---|---|---|---|---|
| **Littrs** | strict (no ambient access) | ~1ms | Rust, Python | instruction + recursion caps | built-in | built-in | `cargo add` / `pip install` |
| Docker | good (container isolation) | ~200ms | no (separate process) | cgroups | roll your own | no | daemon + images |
| Pyodide | poor (JS sandbox leaks) | ~2800ms | JS only | hard to enforce | roll your own | host-level only | WASM runtime + 12MB |
| Monty | strict | <0.1ms | Rust, Python, JS | memory + time + allocations | built-in | no | `pip install` |
| Sandboxing services | strict (managed) | ~1000ms | no (API call) | service-managed | API-based | service-managed | API keys + network |
| `exec()` / subprocess | **none** | ~0.1ms | Python only | none | none | no | none |

*Comparison table adapted from [Monty](https://github.com/pydantic/monty).*

**Why Littrs over Docker/services?** Zero infrastructure. No daemon, no containers, no network calls, no API keys. Just a library you import. Ideal for edge deployments, embedded systems, or anywhere you can't run Docker.

**Why Littrs over `exec()`?** Security. `exec()` gives LLM-generated code full access to your filesystem, network, and environment. Littrs gives it access to nothing except the tools you explicitly register.

**Why Littrs over Pyodide?** Startup speed and server-side safety. Pyodide takes seconds to cold-start and wasn't designed for server-side isolation — Python code can escape into the JS runtime.

**Why Littrs over Monty?** Developer experience. Littrs provides a cleaner API — `@sandbox.tool` to register a function, `sandbox(code)` to run it, `sandbox["x"] = val` to inject variables. No boilerplate, no separate input/output declarations, no configuration objects. It also includes built-in WASM isolation for stronger sandboxing when you need it.

## Citation

If you use Littrs in your research, please cite it as:

```bibtex
@software{littrs,
  title = {Littrs: A Minimal, Secure Python Sandbox for AI Agents},
  author = {Chonkie Inc.},
  url = {https://github.com/chonkie-inc/littrs},
  license = {Apache-2.0},
  year = {2025}
}
```
