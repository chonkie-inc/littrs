<div align="center">

![Littrs Logo](https://github.com/chonkie-inc/littrs/blob/main/assets/littrs.png?raw=true)

# Littrs

### A lightweight, embeddable Python sandbox for LLM tool execution.

[![PyPI version](https://img.shields.io/pypi/v/littrs.svg)](https://pypi.org/project/littrs/)
[![Crates.io](https://img.shields.io/crates/v/littrs.svg)](https://crates.io/crates/littrs)
[![License](https://img.shields.io/github/license/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/blob/main/LICENSE)
[![CI](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml/badge.svg)](https://github.com/chonkie-inc/littrs/actions/workflows/ci.yml)
[![GitHub stars](https://img.shields.io/github/stars/chonkie-inc/littrs.svg)](https://github.com/chonkie-inc/littrs/stargazers)

</div>

---

Littrs is a Python sandbox that you embed directly into your Rust or Python application. There's no container to start, no runtime to boot, no network call to make — just a library that executes LLM-generated Python safely, with only the tools you give it.

It was built for a specific workflow: an LLM writes Python code that calls your functions, and you need to run that code without giving it access to anything else. Littrs compiles Python to bytecode and runs it on a stack-based VM with zero ambient capabilities. The only way sandboxed code can interact with the outside world is through tools you explicitly register.

## Installation

```bash
pip install littrs
```

## Quick Start

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

## Resource Limits

Prevent runaway code from consuming unbounded resources:

```python
sandbox.limit(max_instructions=10_000, max_recursion_depth=50)

try:
    sandbox.run("while True: pass")
except RuntimeError as e:
    print(e)  # "Instruction limit exceeded (limit: 10000)"
```

Resource limit errors are **uncatchable** — `try`/`except` in the sandbox code cannot suppress them. This is by design: the host must always be able to regain control.

## Capturing Print Output

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

## Tool Documentation for LLM Prompts

`describe()` auto-generates Python-style signatures and docstrings from registered tools, ready to embed in a system prompt:

```python
print(sandbox.describe())
# def get_weather(city: str, units: str = 'celsius') -> dict:
#     """Get current weather for a city."""
```

## Low-level Registration

If you need to bypass the decorator (e.g. registering a function that takes raw positional args):

```python
def fetch_data(args):
    return {"id": args[0], "name": "Example"}

sandbox.register("fetch_data", fetch_data)
```

## File Mounting

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

## WASM Sandbox (Stronger Isolation)

For stronger isolation, Littrs can run the interpreter inside a WebAssembly guest module with memory isolation and fuel-based computation limits:

```python
from littrs import WasmSandbox, WasmSandboxConfig

config = WasmSandboxConfig().with_fuel(1_000_000).with_max_memory(32 * 1024 * 1024)
sandbox = WasmSandbox(config)

result = sandbox.run("sum(range(100))")
assert result == 4950
```

Littrs does not support third-party packages, classes, closures, `async`/`await`, `finally`, or `match`. See the full list of [supported Python features](https://github.com/chonkie-inc/littrs/blob/main/FEATURES.md).

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
