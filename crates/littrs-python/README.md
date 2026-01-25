# littrs

A minimal, secure Python sandbox for AI agents.

## Installation

```bash
pip install littrs
```

## Quick Start

```python
from littrs import Sandbox, WasmSandbox

# Simple sandbox (fast, in-process)
sandbox = Sandbox()
sandbox.execute("x = 10")
result = sandbox.execute("x * 2")
print(result)  # 20

# WASM sandbox (stronger isolation, recommended for untrusted code)
wasm = WasmSandbox()
result = wasm.execute("sum(range(100))")
print(result)  # 4950
```

## Features

- **Secure by default**: No file system, network, or OS access
- **Two isolation levels**:
  - `Sandbox`: Fast, in-process execution
  - `WasmSandbox`: WebAssembly isolation (recommended for untrusted code)
- **Tool registration**: Register Python functions callable from the sandbox
- **Resource limits**: Control computation (fuel) and memory usage

## Registering Tools

```python
from littrs import Sandbox

def fetch_data(args):
    id = args[0] if args else 0
    return {"id": id, "name": "Example"}

sandbox = Sandbox()
sandbox.register_function("fetch_data", fetch_data)

result = sandbox.execute("fetch_data(42)")
print(result)  # {'id': 42, 'name': 'Example'}
```

## WASM Sandbox with Limits

```python
from littrs import WasmSandbox, WasmSandboxConfig

# Configure resource limits
config = WasmSandboxConfig() \
    .with_fuel(1_000_000) \
    .with_max_memory(32 * 1024 * 1024)  # 32MB

sandbox = WasmSandbox(config)

# This will run out of fuel and raise an error
try:
    sandbox.execute("while True: pass")
except RuntimeError as e:
    print("Stopped:", e)
```

## Supported Python Features

- Basic types: `None`, `bool`, `int`, `float`, `str`, `list`, `dict`
- Arithmetic, comparison, and boolean operators
- Control flow: `if`/`elif`/`else`, `for`, `while`
- Built-in functions: `len()`, `range()`, `str()`, `int()`, `print()`, etc.

## Not Supported

- `import` statements
- Class definitions
- File I/O
- Network access
- Any standard library

## License

Apache-2.0
