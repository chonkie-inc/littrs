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

* **Run a reasonable subset of Python** — variables, control flow, functions (with defaults, `*args`, `**kwargs`), list comprehensions, f-strings, try/except, and all the built-in types an LLM needs
* **Completely block access to the host environment** — no filesystem, no network, no environment variables, no `import`, no standard library. The sandbox has zero ambient capabilities
* **Call functions on the host** — only functions you explicitly register as tools. The LLM code calls them like normal Python functions, and you handle them in Rust or Python
* **Control resource usage** — set instruction limits and recursion depth limits per execution call. Resource limit violations are uncatchable (they bypass `try`/`except`)
* **Capture stdout** — `print()` output is collected and returned to the caller
* **Be called from Rust or Python** — native Rust API with PyO3 bindings for Python. A WASM guest module is also available for stronger isolation
* **Generate tool documentation for LLMs** — auto-generate Python-style function signatures and docstrings from registered tools, suitable for embedding in system prompts
* **Start up fast** — no interpreter boot, no WASM runtime to load (unless you want it). Create a `Sandbox`, register tools, execute code

## What Littrs cannot do

* Use the standard library — there is no `import`. No `os`, `sys`, `json`, `re`, or anything else
* Use third-party libraries — no `pip install`, no `numpy`, no `requests`
* Define classes — `class` definitions are not supported
* Use async/await — no coroutines, no `asyncio`
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

Littrs can be called from Rust or Python.

### Rust

The core API is the `Sandbox`. Create one, optionally register tools, and call `execute()`:

```rust
use littrs::{Sandbox, PyValue};

let mut sandbox = Sandbox::new();

// Register a tool that the LLM code can call
sandbox.register_fn("fetch_data", |args| {
    let id = args[0].as_int().unwrap_or(0);
    PyValue::Dict(vec![
        ("id".to_string(), PyValue::Int(id)),
        ("name".to_string(), PyValue::Str("Example".to_string())),
    ])
});

// Execute LLM-generated code
let result = sandbox.execute(r#"
data = fetch_data(42)
data["name"]
"#).unwrap();

assert_eq!(result, PyValue::Str("Example".to_string()));
```

Variables persist across `execute()` calls on the same sandbox:

```rust
sandbox.execute("x = 10").unwrap();
sandbox.execute("y = 20").unwrap();
let result = sandbox.execute("x + y").unwrap();
assert_eq!(result, PyValue::Int(30));
```

#### Resource Limits

Prevent runaway code from consuming unbounded resources:

```rust
use littrs::{Sandbox, ResourceLimits};

let mut sandbox = Sandbox::new();
sandbox.set_limits(ResourceLimits {
    max_instructions: Some(10_000),   // cap bytecode instructions per execute() call
    max_recursion_depth: Some(50),    // cap call-stack depth
});

// This will return an error, not hang forever
let err = sandbox.execute("while True: pass").unwrap_err();
assert!(err.to_string().contains("Instruction limit"));
```

Resource limit errors are **uncatchable** — `try`/`except` in the sandbox code cannot suppress them. This is by design: the host must always be able to regain control.

#### Tool Documentation

Generate Python-style docs for all registered tools, suitable for including in an LLM's system prompt:

```rust
use littrs::{Sandbox, ToolInfo, PyValue};

let mut sandbox = Sandbox::new();

sandbox.register_tool(
    ToolInfo::new("get_weather", "Get the current weather for a city")
        .arg_required("city", "str", "The city name")
        .arg_optional("units", "str", "Temperature units (C or F)")
        .returns("dict"),
    |args| {
        let city = args[0].as_str().unwrap_or("Unknown");
        PyValue::Dict(vec![
            ("city".to_string(), PyValue::Str(city.to_string())),
            ("temp".to_string(), PyValue::Int(22)),
        ])
    },
);

let docs = sandbox.describe_tools();
// Produces:
// def get_weather(city: str, units: str = None) -> dict:
//     """Get the current weather for a city"""
```

#### Capturing Print Output

```rust
let mut sandbox = Sandbox::new();
let output = sandbox.execute_with_output(r#"
for i in range(5):
    print(i)
"done"
"#).unwrap();

assert_eq!(output.printed, vec!["0", "1", "2", "3", "4"]);
assert_eq!(output.result, PyValue::Str("done".to_string()));
```

### Python

The Python API mirrors the Rust API:

```python
from littrs import Sandbox

sandbox = Sandbox()

# Register a tool
def fetch_data(args):
    return {"id": args[0], "name": "Example"}

sandbox.register_function("fetch_data", fetch_data)

# Execute LLM-generated code
result = sandbox.execute("""
data = fetch_data(42)
data["name"]
""")
assert result == "Example"
```

#### Resource Limits

```python
sandbox = Sandbox()
sandbox.set_limits(max_instructions=10_000, max_recursion_depth=50)

try:
    sandbox.execute("while True: pass")
except RuntimeError as e:
    print(e)  # "Instruction limit exceeded (limit: 10000)"
```

#### Capturing Print Output

```python
result, printed = sandbox.execute_with_output("""
for i in range(5):
    print(i)
"done"
""")
# printed == ["0", "1", "2", "3", "4"]
# result == "done"
```

#### WASM Sandbox (Stronger Isolation)

For use cases requiring stronger isolation guarantees, Littrs includes a WASM-based sandbox that runs the interpreter inside a WebAssembly guest module. This provides memory isolation and fuel-based computation limits at the WASM level:

```python
from littrs import WasmSandbox, WasmSandboxConfig

config = WasmSandboxConfig().with_fuel(1_000_000).with_max_memory(32 * 1024 * 1024)
sandbox = WasmSandbox(config)

result = sandbox.execute("sum(range(100))")
assert result == 4950
```

## Supported Python Features

Littrs implements enough Python for an LLM to express what it wants to do: call tools, process results, handle errors, and return values.

### Types

`None`, `bool`, `int`, `float`, `str`, `list`, `dict` (string keys)

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

`.upper()`, `.lower()`, `.strip()`, `.split()`, `.join()`, `.replace()`, `.startswith()`, `.endswith()`, `.find()`, `.count()`, `.format()`

### List/Dict Methods

`.append()`, `.pop()`, `.extend()`, `.insert()`, `.remove()`, `.keys()`, `.values()`, `.items()`, `.get()`, `.update()`, `.clear()`

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

1. **Compiler** (`rustpython-parser` AST &rarr; bytecode): Parses Python source into an AST using `rustpython-parser`, then compiles it into a compact bytecode representation (`CodeObject` with ~35 opcodes). Only the compiler depends on the parser crate.

2. **VM** (bytecode &rarr; result): A stack-based virtual machine executes the bytecode. It maintains a value stack, call frames with locals, an exception stack, and global variables that persist across `execute()` calls.

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
