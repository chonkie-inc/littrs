"""littrs - A minimal, secure Python sandbox for AI agents."""

from .littrs import Sandbox as _RustSandbox, WasmSandbox, WasmSandboxConfig
import inspect


class Sandbox:
    """A secure Python sandbox with ``@sandbox.tool`` decorator support.

    Example::

        from littrs import Sandbox

        sandbox = Sandbox()

        @sandbox.tool
        def add(a: int, b: int) -> int:
            '''Add two numbers.'''
            return a + b

        result = sandbox.run("add(2, 3)")
        assert result == 5
    """

    def __init__(self):
        self._inner = _RustSandbox()
        self._tools: dict[str, dict] = {}

    def __getattr__(self, name):
        # Delegate run, capture, set, limit, register, describe
        # to the Rust sandbox.
        return getattr(self._inner, name)

    def __call__(self, code: str):
        """Allow sandbox(code) as shorthand for sandbox.run(code)."""
        return self._inner.run(code)

    def __setitem__(self, name: str, value):
        """Allow sandbox["x"] = val as shorthand for sandbox.set("x", val)."""
        self._inner.set(name, value)

    def tool(self, func=None, *, name=None):
        """Register a function as a sandbox tool.

        The decorated function keeps its normal Python signature.
        Inside the sandbox, LLM-generated code calls it like a
        regular function — argument unpacking is automatic.

        Usage::

            @sandbox.tool
            def peek(start: int = 0, end: int = 1) -> str:
                '''Read chunks [start:end] of the context.'''
                return "\\n".join(chunks[start:end])

            @sandbox.tool(name="search")
            def grep_chunks(pattern: str) -> list:
                '''Return chunk indices matching pattern.'''
                return [i for i, c in enumerate(chunks) if pattern in c]
        """

        def _register(fn):
            tool_name = name or fn.__name__
            sig = inspect.signature(fn)
            params = list(sig.parameters.values())

            def wrapper(args):
                kwargs = {}
                for i, p in enumerate(params):
                    if i < len(args):
                        kwargs[p.name] = args[i]
                    elif p.default is not inspect.Parameter.empty:
                        kwargs[p.name] = p.default
                    else:
                        raise TypeError(
                            f"{tool_name}() missing required argument: '{p.name}'"
                        )
                return fn(**kwargs)

            self._inner.register(tool_name, wrapper)
            self._tools[tool_name] = {
                "sig": sig,
                "doc": inspect.getdoc(fn),
            }
            return fn

        if func is not None:
            # @sandbox.tool  (no parentheses)
            return _register(func)
        # @sandbox.tool(name="...")  (with parentheses)
        return _register

    def describe(self) -> str:
        """Generate Python-style tool docs for LLM system prompts.

        Combines tools registered via Rust ``register()`` and
        Python ``@sandbox.tool``.
        """
        parts = []

        rust_desc = self._inner.describe()
        if rust_desc.strip():
            parts.append(rust_desc)

        for tool_name, meta in self._tools.items():
            sig = meta["sig"]
            doc = meta["doc"] or ""

            param_strs = []
            for p in sig.parameters.values():
                s = p.name
                if p.annotation is not inspect.Parameter.empty:
                    s += f": {getattr(p.annotation, '__name__', str(p.annotation))}"
                if p.default is not inspect.Parameter.empty:
                    s += f" = {p.default!r}"
                param_strs.append(s)

            ret = ""
            if sig.return_annotation is not inspect.Signature.empty:
                ret_name = getattr(
                    sig.return_annotation, "__name__", str(sig.return_annotation)
                )
                ret = f" -> {ret_name}"

            parts.append(
                f"def {tool_name}({', '.join(param_strs)}){ret}:\n"
                f'    """{doc}"""'
            )

        return "\n\n".join(parts)


__all__ = ["Sandbox", "WasmSandbox", "WasmSandboxConfig"]
