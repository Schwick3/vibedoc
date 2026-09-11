"""Conservative source-only Python facts; never imports the analyzed workspace."""
import ast
import json
from pathlib import Path
import sys
import tokenize

VERSION = "0.1.2"
BUILTINS = {"bool", "str", "int", "float", "bytes", "list", "dict", "tuple", "set", "frozenset", "object"}
SKIP = {".git", ".venv", "venv", "__pycache__", "node_modules", "target", "dist"}
FUNCTIONS = (ast.FunctionDef, ast.AsyncFunctionDef)


def location(path, node):
    return {"path": path, "range": {
        "start": {"line": node.lineno, "column": node.col_offset + 1},
        "end": {"line": node.end_lineno, "column": node.end_col_offset + 1}}}


def bindings(tree):
    # Deliberately over-approximate shadowing, including nested scopes.
    names = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Name) and isinstance(node.ctx, (ast.Store, ast.Del)):
            names.add(node.id)
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            names.add(node.name)
        elif isinstance(node, ast.arg):
            names.add(node.arg)
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            names.update(alias.asname or alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.ExceptHandler) and node.name:
            names.add(node.name)
    return names


def type_fact(annotation, shadowed):
    def supported(node):
        if isinstance(node, ast.Name):
            return node.id in BUILTINS and node.id not in shadowed
        if isinstance(node, ast.Constant):
            return node.value is None
        if isinstance(node, ast.Subscript):
            if not isinstance(node.value, ast.Name) or not supported(node.value):
                return False
            arguments = node.slice.elts if isinstance(node.slice, ast.Tuple) else [node.slice]
            arity = {"list": 1, "dict": 2, "set": 1, "frozenset": 1}.get(node.value.id)
            if node.value.id != "tuple" and len(arguments) != arity:
                return False
            return bool(arguments) and all(supported(item) for item in arguments)
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
            return supported(node.left) and supported(node.right)
        return False

    display = ast.unparse(annotation) if annotation else "unknown"
    return {"display": display, "normalized": display,
            "confidence": "exact" if annotation and "*" not in shadowed and supported(annotation) else "incomplete"}


def analyze_file(path, text):
    tree = ast.parse(text, filename=path)
    shadowed = bindings(tree)
    symbols = {}
    definitions = {}

    def visit(body, prefix="", uncertain=False, in_class=False):
        for node in body:
            if isinstance(node, (*FUNCTIONS, ast.ClassDef)):
                name = prefix + node.name
                definitions[name] = definitions.get(name, 0) + 1
            if isinstance(node, ast.ClassDef):
                visit(node.body, prefix + node.name + ".",
                      uncertain or bool(node.decorator_list or node.bases or node.keywords),
                      True)
            elif isinstance(node, FUNCTIONS):
                qualified = prefix + node.name
                decorators = [d.id if isinstance(d, ast.Name) else None for d in node.decorator_list]
                known_method = (in_class and len(decorators) == 1
                                and decorators[0] in {"staticmethod", "classmethod"}
                                and decorators[0] not in shadowed)
                incomplete = (uncertain or bool(decorators) and not known_method
                              or bool(getattr(node, "type_params", [])))
                positional = node.args.posonlyargs + node.args.args
                defaults = [None] * (len(positional) - len(node.args.defaults)) + node.args.defaults
                arguments = list(zip(positional, defaults, [False] * len(positional)))
                if in_class and decorators != ["staticmethod"] and arguments:
                    arguments = arguments[1:]
                if node.args.vararg:
                    arguments.append((node.args.vararg, None, True))
                arguments.extend(zip(node.args.kwonlyargs, node.args.kw_defaults,
                                     [False] * len(node.args.kwonlyargs)))
                if node.args.kwarg:
                    arguments.append((node.args.kwarg, None, True))
                parameters = [{
                    "name": arg.arg, "typeFact": type_fact(arg.annotation, shadowed),
                    "optional": default is not None or rest, "rest": rest,
                    "destructured": False, "location": location(path, arg),
                } for arg, default, rest in arguments]
                returns = type_fact(node.returns, shadowed)
                # A generator's return annotation is not its yielded-value type.
                if any(isinstance(n, (ast.Yield, ast.YieldFrom)) for n in ast.walk(node)):
                    returns["confidence"] = "incomplete"
                signature = {"parameters": parameters, "returnType": returns,
                             "declaration": location(path, node)}
                if qualified in symbols:
                    symbols[qualified]["signatures"].append(signature)
                    symbols[qualified]["confidence"] = "incomplete"
                else:
                    symbols[qualified] = {
                        "id": f"python:{path}#{qualified}", "adapter": "python",
                        "language": "python", "name": node.name, "qualifiedName": qualified,
                        "kind": "method" if in_class else "function",
                        "exported": not any(p.startswith("_") for p in qualified.split(".")),
                        "declaration": location(path, node), "signatures": [signature],
                        "throws": [], "confidence": "incomplete" if incomplete else "exact",
                    }
                # Nested functions do not have stable public bindings.
            else:
                # Conditional definitions are exposed, but never claimed exact.
                for _, value in ast.iter_fields(node):
                    if isinstance(value, list) and value and isinstance(value[0], ast.stmt):
                        visit(value, prefix, True, in_class)

    visit(tree.body)
    # Assignments can replace a declared callable after its definition.
    assigned = {n.id for n in ast.walk(tree)
                if isinstance(n, ast.Name) and isinstance(n.ctx, (ast.Store, ast.Del))}
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            assigned.update(alias.asname or alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.Attribute) and isinstance(node.ctx, (ast.Store, ast.Del)):
            assigned.add(node.attr)
    for symbol in symbols.values():
        parts = symbol["qualifiedName"].split(".")
        repeated = any(definitions.get(".".join(parts[:i]), 0) > 1
                       for i in range(1, len(parts) + 1))
        if "*" in assigned or repeated or any(part in assigned for part in parts):
            symbol["confidence"] = "incomplete"
    return sorted(symbols.values(), key=lambda s: s["id"])


def analyze(params):
    root = Path(params["workspaceRoot"]).resolve(strict=True)
    if params.get("projects"):
        raise ValueError("Python uses sourceGlobs, not project configurations")
    patterns = params.get("sourceGlobs") or ["**/*.py"]
    if not isinstance(patterns, list) or not all(isinstance(p, str) for p in patterns):
        raise ValueError("sourceGlobs must be a list of strings")
    paths = set()
    for pattern in patterns:
        if Path(pattern).is_absolute() or ".." in Path(pattern).parts:
            raise ValueError("sourceGlobs must stay within the workspace")
        for path in root.glob(pattern):
            relative = path.relative_to(root)
            if any(part in SKIP for part in relative.parts) or path.suffix != ".py":
                continue
            # Refuse symlink escapes, including symlinked ancestor directories.
            if path.is_file() and path.resolve().is_relative_to(root):
                paths.add(path)
    graph = {"files": [], "symbols": [], "relationships": []}
    diagnostics = []
    for path in sorted(paths):
        relative = path.relative_to(root).as_posix()
        try:
            with tokenize.open(path) as stream:
                symbols = analyze_file(relative, stream.read())
            graph["files"].append({"path": relative, "language": "python"})
            graph["symbols"].extend(symbols)
        except (SyntaxError, UnicodeError, OSError, ValueError, RecursionError) as error:
            diagnostics.append({"code": "PY001", "severity": "error",
                                "message": f"{relative}: {error}"})
    if not paths:
        diagnostics.append({"code": "PY002", "severity": "error",
                            "message": "No Python source files matched sourceGlobs"})
    return {"graph": graph, "diagnostics": diagnostics}


class RpcError(ValueError):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def serve():
    for line in sys.stdin:
        request_id = None
        stop = False
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                raise RpcError(-32600, "Invalid JSON-RPC request")
            request_id = request.get("id")
            if (request.get("jsonrpc") != "2.0" or type(request_id) is not int
                    or not isinstance(request.get("method"), str)):
                raise RpcError(-32600, "Invalid JSON-RPC request")
            method, params = request["method"], request.get("params", {})
            if not isinstance(params, dict):
                raise RpcError(-32602, "params must be an object")
            if method == "initialize":
                if params.get("protocolVersion") != 1:
                    raise RpcError(-32001, "Unsupported protocol version")
                result = {"protocolVersion": 1,
                          "adapter": {"name": "python", "version": VERSION,
                                      "runtime": f"python {sys.version.split()[0]}"},
                          "capabilities": {"languages": ["python"], "extensions": [".py"],
                                           "relationships": []}}
            elif method == "analyze":
                result = analyze(params)
            elif method == "shutdown":
                result, stop = {}, True
            else:
                raise RpcError(-32601, f"Unknown method: {method}")
            response = {"jsonrpc": "2.0", "id": request_id, "result": result}
        except Exception as error:
            response = {"jsonrpc": "2.0", "id": request_id,
                        "error": {"code": -32700 if isinstance(error, json.JSONDecodeError) else getattr(error, "code", -32000),
                                  "message": str(error)}}
        print(json.dumps(response), flush=True)
        if stop:
            break


if __name__ == "__main__":
    if sys.argv[1:] != ["--stdio"]:
        sys.exit("vibedoc-adapter-python must be run with --stdio")
    serve()
