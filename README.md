<div align="center">

![Littrs Logo](https://github.com/chonkie-inc/littrs/blob/main/assets/littrs.png?raw=true)

# Littrs

### A lightweight, embeddable Python sandbox for LLM tool execution.

[![Crates.io](https://img.shields.io/crates/v/littrs.svg)](https://crates.io/crates/littrs)
[![PyPI version](https://img.shields.io/pypi/v/littrs.svg)](https://pypi.org/project/littrs/)
[![License](https://img.shields.io/github/license/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/blob/main/LICENSE)
[![CI](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml/badge.svg)](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml)
[![GitHub stars](https://img.shields.io/github/stars/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/stargazers)

</div>

---

Littrs is a Python sandbox that you embed directly into your Rust or Python application. There's no container to start, no runtime to boot, no network call to make — just a library that executes LLM-generated Python safely, with only the tools you give it.

It was built for a specific workflow: an LLM writes Python code that calls your functions, and you need to run that code without giving it access to anything else. Littrs compiles Python to bytecode and runs it on a stack-based VM with zero ambient capabilities. The only way sandboxed code can interact with the outside world is through tools you explicitly register.

* **Stateful sandbox with tool registration** — register Python or Rust functions as tools via `@sandbox.tool` / `#[tool]`, inject variables, and run multiple code snippets against the same state
* **Zero ambient capabilities** — no filesystem, no network, no env vars, no `import`. Sandboxed code can only call tools you register
* **Resource limits** — cap bytecode instructions and recursion depth per `run()` call. Limits are enforced at the VM level and cannot be caught by `try`/`except`
* **Stdout capture** — `print()` output is collected and returned separately from the result
* **Auto-generated tool docs** — `describe()` produces Python-style signatures and docstrings from registered tools, ready to paste into a system prompt
* **Rust and Python APIs** — native Rust with PyO3 bindings. Optional WASM isolation via an embedded wasmtime guest module
* **Fast startup** — no interpreter boot. Create a sandbox, register tools, run code

Littrs does not support `import`, third-party packages, classes, closures, `async`/`await`, `finally`, or `match` — see the [ROADMAP](ROADMAP.md) for what's planned and the full list of [supported Python features](FEATURES.md).

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
