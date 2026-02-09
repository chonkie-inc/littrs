# Supported Python Features

Littrs implements enough Python for an LLM to express what it wants to do: call tools, process results, handle errors, and return values.

## Types

`None`, `bool`, `int`, `float`, `str`, `list`, `tuple`, `dict`, `set`

## Operators

| Category | Operators |
|----------|-----------|
| Arithmetic | `+`, `-`, `*`, `/`, `//`, `%`, `**` |
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`, `is`, `is not` |
| Boolean | `and`, `or`, `not` |
| Bitwise | `\|`, `^`, `&`, `<<`, `>>`, `~` |
| Assignment | `=`, `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=` |

## Control Flow

- `if`/`elif`/`else`
- `for` loops over lists, strings, ranges, `dict.items()`, etc. — with `break`/`continue`
- `while` loops with `break`/`continue`
- Ternary expressions: `x if condition else y`
- List comprehensions with filters: `[x*2 for x in items if x > 0]`

## Functions

- `def` with positional parameters, default values, `*args`, `**kwargs`
- `lambda` expressions: `lambda x, y: x + y`
- Keyword arguments at call sites: `f(x=1, y=2)`
- Recursive and nested function definitions
- Implicit `return None` for functions without a return statement

## Error Handling

- `try`/`except` with typed handlers: `except ValueError as e:`
- Bare `except:` to catch all exceptions
- `else` clause on try blocks
- `raise ValueError("message")` and bare `raise` to re-raise

## F-strings

```python
name = "world"
f"hello {name}!"  # "hello world!"
```

## String Methods

`.upper()`, `.lower()`, `.strip()`, `.lstrip()`, `.rstrip()`, `.split()`, `.join()`, `.replace()`, `.startswith()`, `.endswith()`, `.find()`, `.count()`, `.title()`, `.capitalize()`, `.isdigit()`, `.isalpha()`, `.isalnum()`

## List/Dict/Set Methods

`.append()`, `.pop()`, `.extend()`, `.insert()`, `.remove()`, `.index()`, `.count()`, `.keys()`, `.values()`, `.items()`, `.get()`, `.update()`, `.clear()`, `.add()`, `.discard()`, `.union()`, `.intersection()`, `.difference()`

## Slicing

```python
items = [1, 2, 3, 4, 5]
items[1:3]    # [2, 3]
items[::2]    # [1, 3, 5]
items[::-1]   # [5, 4, 3, 2, 1]
```

## Imports

- `import module` / `import module as alias`
- `from module import name` / `from module import name as alias`
- Custom module registration via `sandbox.module()`

### Built-in Modules

Available when using `Sandbox::with_builtins()` (Rust) or `Sandbox(builtins=True)` (Python):

| Module | Contents |
|--------|----------|
| `json` | `loads(s)`, `dumps(obj)` |
| `math` | `pi`, `e`, `inf`, `nan`, `tau`, `sqrt`, `floor`, `ceil`, `log`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `fabs`, `pow`, `exp`, `isnan`, `isinf`, `degrees`, `radians`, `trunc`, `gcd`, `factorial` |
| `typing` | `Any`, `Union`, `Optional`, `List`, `Dict`, `Tuple`, `Set`, `Callable`, `Type`, `Literal`, `TypeVar`, `Generic`, `Protocol`, `NamedTuple`, `TypedDict`, and more (all no-ops at runtime) |

## Built-in Functions

`len()`, `str()`, `int()`, `float()`, `bool()`, `list()`, `range()`, `abs()`, `min()`, `max()`, `sum()`, `print()`, `type()`, `isinstance()`, `enumerate()`, `zip()`, `sorted()`, `reversed()`, `dict()`, `tuple()`, `set()`, `round()`, `map()`, `filter()`, `any()`, `all()`, `chr()`, `ord()`
