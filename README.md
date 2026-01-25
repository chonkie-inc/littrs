# littrs

A minimal, secure Python sandbox for AI agents.

## Features

- **Secure by default**: No file system, network, or OS access
- **Minimal Python subset**: Variables, control flow, functions, basic types
- **Tool registration**: Register Rust functions callable from Python
- **Fast startup**: Millisecond-level initialization

## Quick Start

```rust
use littrs::{Sandbox, PyValue};

// Create a sandbox
let mut sandbox = Sandbox::new();

// Register a tool
sandbox.register_fn("fetch_data", |args| {
    let id = args[0].as_int().unwrap_or(0);
    PyValue::Dict(vec![
        ("id".to_string(), PyValue::Int(id)),
        ("name".to_string(), PyValue::Str("Example".to_string())),
    ])
});

// Execute code
let result = sandbox.execute(r#"
data = fetch_data(42)
data
"#).unwrap();
```

## License

Apache-2.0
