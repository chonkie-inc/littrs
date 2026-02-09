# Roadmap

This document tracks Python features that Littrs does not yet support, roughly ordered by how often LLMs generate code that uses them. Items near the top are things LLMs reach for constantly; items near the bottom are rarely needed in a sandbox context.

Contributions welcome on any of these.

---

## Language Features

### ~~Lambda functions~~ ✅

- [x] Compile `lambda` expressions to `MakeFunction` + inline `CodeObject`
- [x] Support `key=` parameter in `sorted()` (with `reverse=` too)
- [x] Support `key=` and `reverse=` parameters in `list.sort()`

### ~~Dict and set comprehensions~~ ✅

- [x] Dict comprehensions: `{k: v for k, v in items if condition}`
- [x] Set comprehensions: `{x for x in items}` (depends on set type — already done)

### Closures and nested scopes

Functions defined inside other functions cannot currently capture variables from the enclosing scope. This means patterns like helper functions, callbacks, and any form of functional composition break silently or error. LLMs generate closures often — especially when building up data transformers or when told to "write a function that returns a function."

- [ ] Track free variables during compilation (scope analysis pass)
- [ ] Add cell/freevar storage to `CallFrame`
- [ ] Capture environment when creating function values

### `global` and `nonlocal` declarations

Without `global`, assigning to a variable inside a function always creates a local. Without `nonlocal`, inner functions cannot mutate variables from an enclosing function scope. These are needed for correct Python scoping semantics and are a prerequisite for closures working properly.

- [ ] `global` statement: mark variables to read/write from module globals
- [ ] `nonlocal` statement: mark variables to read/write from enclosing function scope

### `for`/`else` and `while`/`else`

Python's `for`/`else` runs the `else` block only when the loop completes without hitting `break`. LLMs use this pattern for search-and-bail logic: "iterate looking for X, and if you don't find it, do Y." Not extremely common, but when an LLM reaches for it and it fails, the error is confusing.

- [ ] `for`/`else`: run else block when loop exits normally (no `break`)
- [ ] `while`/`else`: same semantics for while loops

### ~~`assert` statement~~ ✅

- [x] `assert condition`
- [x] `assert condition, "message"`

### `try`/`finally`

The `finally` block guarantees cleanup code runs whether or not an exception occurred. LLMs sometimes generate try/finally for resource cleanup patterns. Currently only `try`/`except`/`else` is supported.

- [ ] Compile `finally` blocks
- [ ] Ensure finally runs on normal exit, exception, `break`, `continue`, and `return`

### Exception chaining (`raise X from Y`)

Python's `raise ValueError("bad") from original_error` attaches a cause to the new exception. Not critical for sandbox use, but LLMs do generate it when wrapping errors. Currently raises an `Unsupported` error.

- [ ] Parse and compile `raise X from Y`
- [ ] Store `__cause__` on exception values

### Exception hierarchy

Currently, `except ValueError` only catches errors whose type name is exactly `"ValueError"`. In real Python, `except LookupError` catches both `KeyError` and `IndexError`, and `except Exception` catches almost everything. This matters when LLMs write broad exception handlers.

- [ ] Define parent/child relationships between exception types
- [ ] `except ArithmeticError` catches `ZeroDivisionError`, `OverflowError`
- [ ] `except LookupError` catches `KeyError`, `IndexError`
- [ ] `except Exception` catches all standard exceptions

---

## Types

### ~~Set~~ ✅ and frozenset

- [x] `PyValue::Set` type
- [x] Set literals: `{1, 2, 3}`
- [x] `set()` builtin constructor
- [x] Methods: `add`, `remove`, `discard`, `pop`, `union`, `intersection`, `difference`, `symmetric_difference`, `issubset`, `issuperset`, `clear`, `update`
- [x] `in` / `not in` membership testing
- [x] `isdisjoint`, `copy` methods
- [ ] `frozenset()` (immutable variant)

### ~~Real tuple type~~ ✅

- [x] `PyValue::Tuple` distinct from `PyValue::List`
- [x] Tuple immutability (reject index assignment)
- [x] `tuple()` builtin constructor
- [x] Methods: `count`, `index`
- [x] Hashable tuples (usable as dict keys)

### ~~Non-string dict keys~~ ✅

- [x] Support `int`, `bool`, `None`, `float`, `tuple` as dict keys
- [x] Switched from `Vec<(String, PyValue)>` to `Vec<(PyValue, PyValue)>`

### Bytes type

Byte strings (`b"hello"`) are used for binary data handling. Not common in LLM-generated sandbox code, but occasionally needed when tools return binary data.

- [ ] `PyValue::Bytes` type
- [ ] `bytes()` constructor
- [ ] Basic methods: `decode`, `hex`, `find`, `count`

### Big integers

Python integers have arbitrary precision. Littrs uses `i64`, which overflows at ~9.2 quintillion. Most LLM code stays well within i64 range, but edge cases with large factorials, combinatorics, or crypto-adjacent math will silently overflow or error.

- [ ] Arbitrary-precision integer support (e.g., via `num-bigint`)
- [ ] Seamless promotion from i64 when overflow is detected

---

## Built-in Functions

### ~~Missing builtins~~ (partially ✅)

Several commonly-used builtins are now implemented:

- [x] `repr(x)` — string representation (LLMs use this for debugging and logging)
- [x] `bin(n)`, `hex(n)`, `oct(n)` — number formatting
- [x] `divmod(a, b)` — returns `(a // b, a % b)`
- [x] `pow(base, exp, mod=None)` — power with optional modulus
- [x] `hash(x)` — hash value (needed if sets/frozensets are added)
- [ ] `id(x)` — object identity (can be a no-op or return a placeholder)
- [ ] `next(iterator, default)` — advance an iterator
- [ ] `input()` — not applicable in sandbox, but could return empty string or error clearly

### `isinstance` with type objects

Currently `isinstance(x, "str")` takes a string typename. Real Python uses type objects: `isinstance(x, str)`, `isinstance(x, (int, float))`. LLMs almost always write the real Python form.

- [ ] Accept type objects (or at least bare type names) as the second argument
- [ ] Support tuples of types: `isinstance(x, (int, float))`

---

## Methods

### ~~Missing string methods~~ (partially ✅)

Many commonly-used string methods are now implemented:

- [x] `removeprefix(prefix)`, `removesuffix(suffix)` — Python 3.9+, LLMs use these
- [x] `partition(sep)`, `rpartition(sep)` — split into 3-tuple at first/last occurrence
- [x] `splitlines(keepends=False)` — split by line boundaries
- [x] `center(width, fillchar)`, `ljust(width, fillchar)`, `rjust(width, fillchar)` — padding
- [x] `zfill(width)` — zero-pad numbers
- [x] `swapcase()`, `casefold()` — case transformations
- [ ] `rsplit(sep, maxsplit)` — split from the right
- [ ] `rfind(sub)`, `rindex(sub)` — search from the right
- [ ] `isspace()`, `islower()`, `isupper()`, `isascii()`, `isdecimal()`, `isidentifier()`, `istitle()` — predicates
- [ ] `encode(encoding)` — string to bytes (depends on bytes type)

### ~~`str.format()`~~ (partially ✅)

Basic positional and indexed substitution is now supported:

- [x] Basic positional: `"{} {}".format(a, b)`
- [x] Indexed: `"{0} {1}".format(a, b)`
- [x] Escaped braces: `"{{literal}}".format()`
- [ ] Keyword: `"{name}".format(name=x)` (requires kwargs on `CallMethod`)

### ~~`sorted()` with `key=` and `reverse=`~~ ✅

- [x] `reverse=True` parameter on `sorted()`
- [x] `key=func` parameter on `sorted()` (works with lambdas)
- [x] `key=` and `reverse=` on `list.sort()`

---

## F-string Enhancements

### Format specifications

F-strings currently support basic interpolation (`f"{value}"`) but not format specs. LLMs generate `f"{price:.2f}"`, `f"{name:>20}"`, `f"{count:04d}"` for number formatting and alignment.

- [ ] Format specs: `f"{value:.2f}"`, `f"{value:>10}"`, `f"{value:04d}"`

### Conversion flags

Python f-strings support `!s` (str), `!r` (repr), and `!a` (ascii) conversion flags. LLMs sometimes use `f"{value!r}"` for debug output.

- [ ] `!s`, `!r`, `!a` conversion flags

---

## ~~Virtual Filesystem~~ ✅

- [x] `sandbox.mount(virtual_path, host_path, writable)` — mount host files into the sandbox
- [x] `open(path)` / `open(path, "w")` builtin — read/write mounted files
- [x] File methods: `.read()`, `.readline()`, `.readlines()`, `.write(s)`, `.close()`
- [x] Write-through: writes persist to host path on `.write()` and `.close()`
- [x] `sandbox.files()` — inspect current writable file contents from the host
- [x] `FileNotFoundError` for unmounted paths, `PermissionError` for read-only writes
- [x] `UnsupportedOperation` for mode mismatches, `ValueError` for closed files
- [x] All file errors catchable with `try`/`except`
- [ ] Context manager support: `with open("f") as f:` (depends on `with` statement)
- [ ] `"a"` (append) mode
- [ ] `"r+"` / `"w+"` (read-write) modes

---

## Low Priority

These features are rarely needed in a sandbox context but are listed for completeness.

### `match` statements

Python 3.10 structural pattern matching. Some newer LLMs generate `match`/`case` syntax, but it's still uncommon compared to if/elif chains.

### `del` statement

Deleting variables and collection items. Rarely needed by LLM code.

### Decorators

`@decorator` syntax for functions. Not useful without a module system or classes.

### `class` definitions

Full class support would be a major undertaking and is explicitly out of scope for the sandbox use case. LLMs can work effectively with dicts and functions.

### `async`/`await`

Coroutines and asynchronous execution. Out of scope — the sandbox runs synchronously to completion.

### Walrus operator (`:=`)

Named expressions like `if (n := len(items)) > 10:`. Occasionally generated by LLMs but easy to work around with a separate assignment.
