<div align="center">

![Littrs Logo](https://github.com/chonkie-inc/littrs/blob/main/assets/littrs.png?raw=true)

# Littrs

### A minimal, secure Python sandbox written in Rust for use by AI agents.

[![Crates.io](https://img.shields.io/crates/v/littrs.svg)](https://crates.io/crates/littrs)
[![PyPI version](https://img.shields.io/pypi/v/littrs.svg)](https://pypi.org/project/littrs/)
[![License](https://img.shields.io/github/license/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/blob/main/LICENSE)
[![CI](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml/badge.svg)](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml)
[![GitHub stars](https://img.shields.io/github/stars/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/stargazers)

</div>

---

A minimal, secure Python sandbox written in Rust for use by AI agents.

Littrs avoids the cost, latency, and complexity of using full container-based sandboxes for running LLM-generated code. Instead, it lets you safely run Python code written by an LLM embedded directly in your agent, with startup times measured in milliseconds and zero external dependencies.

The core idea is simple: LLMs work faster, cheaper, and more reliably when they write Python code instead of relying on traditional structured tool calling. Littrs makes that possible without spinning up containers or risking arbitrary code execution on the host. You register Rust (or Python) functions as callable tools, hand the sandbox some LLM-generated code, and get back a result — safely.

For motivation on why you might want to do this, see:
* [Programmatic Tool Calling](https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling) from Anthropic
* [Code Execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp) from Anthropic
* [Codemode](https://blog.cloudflare.com/code-mode/) from Cloudflare
* [Smol Agents](https://github.com/huggingface/smolagents) from Hugging Face

## What Littrs can do

* **Run a reasonable subset of Python** — variables, control flow, functions (with defaults, `*args`, `**kwargs`), lambdas, list comprehensions, f-strings, try/except, and all the built-in types an LLM needs
* **Completely block access to the host environment** — no filesystem, no network, no environment variables, no `import`, no standard library. The sandbox has zero ambient capabilities
* **Call functions on the host** — only functions you explicitly register as tools. The LLM code calls them like normal Python functions, and you handle them in Rust or Python
* **Control resource usage** — set instruction limits and recursion depth limits per run call. Resource limit violations are uncatchable (they bypass `try`/`except`)
* **Capture stdout** — `print()` output is collected and returned to the caller
* **Be called from Rust or Python** — native Rust API with PyO3 bindings for Python. A WASM guest module is also available for stronger isolation
* **Generate tool documentation for LLMs** — auto-generate Python-style function signatures and docstrings from registered tools, suitable for embedding in system prompts
* **Start up fast** — no interpreter boot, no WASM runtime to load (unless you want it). Create a `Sandbox`, register tools, run code

## What Littrs cannot do

* Use the standard library — there is no `import`. No `os`, `sys`, `json`, `re`, or anything else
* Use third-party libraries — no `pip install`, no `numpy`, no `requests`
* Define classes — `class` definitions are not supported
* Use async/await — no coroutines, no `asyncio`
* Use closures (functions cannot capture variables from enclosing scopes)
* Use `finally` blocks — only `try`/`except`/`else`
* Use `match` statements
* Snapshot/resume execution state — execution runs to completion in a single call

## Installation

### Rust

```toml
[dependencies]
littrs = "0.4"
```

### Python

```bash
pip install littrs
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

## Supported Python Features

Littrs implements enough Python for an LLM to express what it wants to do: call tools, process results, handle errors, and return values.

### Types

`None`, `bool`, `int`, `float`, `str`, `list`, `tuple`, `dict`, `set`

### Operators

| Category | Operators |
|----------|-----------|
| Arithmetic | `+`, `-`, `*`, `/`, `//`, `%`, `**` |
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`, `is`, `is not` |
| Boolean | `and`, `or`, `not` |
| Bitwise | `\|`, `^`, `&`, `<<`, `>>`, `~` |
| Assignment | `=`, `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=` |

### Control Flow

- `if`/`elif`/`else`
- `for` loops over lists, strings, ranges, `dict.items()`, etc. — with `break`/`continue`
- `while` loops with `break`/`continue`
- Ternary expressions: `x if condition else y`
- List comprehensions with filters: `[x*2 for x in items if x > 0]`

### Functions

- `def` with positional parameters, default values, `*args`, `**kwargs`
- `lambda` expressions: `lambda x, y: x + y`
- Keyword arguments at call sites: `f(x=1, y=2)`
- Recursive and nested function definitions
- Implicit `return None` for functions without a return statement

### Error Handling

- `try`/`except` with typed handlers: `except ValueError as e:`
- Bare `except:` to catch all exceptions
- `else` clause on try blocks
- `raise ValueError("message")` and bare `raise` to re-raise

### F-strings

```python
name = "world"
f"hello {name}!"  # "hello world!"
```

### String Methods

`.upper()`, `.lower()`, `.strip()`, `.lstrip()`, `.rstrip()`, `.split()`, `.join()`, `.replace()`, `.startswith()`, `.endswith()`, `.find()`, `.count()`, `.title()`, `.capitalize()`, `.isdigit()`, `.isalpha()`, `.isalnum()`

### List/Dict/Set Methods

`.append()`, `.pop()`, `.extend()`, `.insert()`, `.remove()`, `.index()`, `.count()`, `.keys()`, `.values()`, `.items()`, `.get()`, `.update()`, `.clear()`, `.add()`, `.discard()`, `.union()`, `.intersection()`, `.difference()`

### Slicing

```python
items = [1, 2, 3, 4, 5]
items[1:3]    # [2, 3]
items[::2]    # [1, 3, 5]
items[::-1]   # [5, 4, 3, 2, 1]
```

### Built-in Functions

`len()`, `str()`, `int()`, `float()`, `bool()`, `list()`, `range()`, `abs()`, `min()`, `max()`, `sum()`, `print()`, `type()`, `isinstance()`, `enumerate()`, `zip()`, `sorted()`, `reversed()`, `dict()`, `tuple()`, `set()`, `round()`, `map()`, `filter()`, `any()`, `all()`, `chr()`, `ord()`

## Architecture

Littrs uses a two-phase execution model:

1. **Compiler** (AST &rarr; bytecode): Parses Python source into an AST using Ruff's `ruff_python_parser`, then compiles it into a compact bytecode representation (`CodeObject` with ~35 opcodes). Only the compiler depends on the parser crate.

2. **VM** (bytecode &rarr; result): A stack-based virtual machine executes the bytecode. It maintains a value stack, call frames with locals, an exception stack, and global variables that persist across `run()` calls.

This separation means parsing and compilation happen once, and the VM is a tight instruction dispatch loop. Exception handling uses a static exception table (same approach as CPython 3.11+) rather than runtime-generated exception frames.

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

**Why Littrs over Docker/services?** Zero infrastructure. No daemon, no containers, no network calls, no API keys. Just a library you import. Ideal for edge deployments, embedded systems, or anywhere you can't run Docker.

**Why Littrs over `exec()`?** Security. `exec()` gives LLM-generated code full access to your filesystem, network, and environment. Littrs gives it access to nothing except the tools you explicitly register.

**Why Littrs over Pyodide?** Startup speed and server-side safety. Pyodide takes seconds to cold-start and wasn't designed for server-side isolation — Python code can escape into the JS runtime.

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
