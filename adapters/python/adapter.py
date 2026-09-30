"""Conservative source-only Python facts; never imports the analyzed workspace."""
import ast
import json
import keyword
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


TYPING_FORMS = {"Optional", "Union", "List", "Dict", "Tuple", "Set", "FrozenSet"}
COLLECTION_ARITY = {"list": 1, "dict": 2, "set": 1, "frozenset": 1}
TYPE_DEPTH = 64
TYPE_NODES = 4096
TYPE_TEXT = 16_384


def binding_counts(tree):
    """Count all possible bindings, conservatively including nested scopes."""
    counts = {}
    mutated = set()
    for node in ast.walk(tree):
        names = []
        if isinstance(node, ast.Name) and isinstance(node.ctx, (ast.Store, ast.Del)):
            names = [node.id]
        elif isinstance(node, (*FUNCTIONS, ast.ClassDef)):
            names = [node.name]
        elif isinstance(node, ast.arg):
            names = [node.arg]
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            names = [a.asname or a.name.split(".")[0] for a in node.names]
        elif isinstance(node, (ast.ExceptHandler, ast.MatchAs, ast.MatchStar)) and node.name:
            names = [node.name]
        elif isinstance(node, ast.MatchMapping) and node.rest:
            names = [node.rest]
        elif isinstance(node, ast.Attribute) and isinstance(node.ctx, (ast.Store, ast.Del)):
            names = [node.attr]
            root = node.value
            while isinstance(root, ast.Attribute):
                root = root.value
            if isinstance(root, ast.Name):
                mutated.add(root.id)
        for name in names:
            counts[name] = counts.get(name, 0) + 1
    return counts, mutated


def module_name(path):
    parts = list(Path(path).with_suffix("").parts)
    if parts[-1] == "__init__":
        parts.pop()
    # A dotted filename is not a Python module segment; do not conflate it
    # with directories. Root __init__.py has no known package identity.
    if not parts or not all(p.isidentifier() and not keyword.iskeyword(p) for p in parts):
        return None
    return ".".join(parts)


def type_union(items):
    members = set()
    for item in items:
        if item[0] == "|":
            members.update(item[1])
        else:
            members.add(item)
    ordered = tuple(sorted(members))
    return ordered[0] if len(ordered) == 1 else ("|", ordered)


def bounded_type(item):
    pending = [item]
    count = 0
    while pending:
        _, children = pending.pop()
        count += 1
        if count > TYPE_NODES:
            return False
        pending.extend(children)
    return True


def render_type(item):
    name, arguments = item
    if name == "|":
        return " | ".join(render_type(arg) for arg in arguments)
    if arguments:
        return name + "[" + ", ".join(render_type(arg) for arg in arguments) + "]"
    return name


class TypeResolver:
    """Resolve only selected source trees. This index performs no filesystem I/O."""
    def __init__(self, trees, selected=None):
        self.trees = trees
        self.selected = set(trees) if selected is None else set(selected)
        self.modules = {}
        self.contexts = {}
        self.cache = {}
        for path in sorted(self.selected):
            name = module_name(path)
            if name:
                self.modules.setdefault(name, []).append(path)
        self.typing_shadowed = any(Path(p).name == "typing.py" or
                                   Path(p).parts[-2:] == ("typing", "__init__.py")
                                   for p in self.selected)
        for path, tree in trees.items():
            counts, mutated = binding_counts(tree)
            shadowed = bindings(tree) | set(counts)
            assigned = rebound_names(tree)
            candidates = {}
            for node in tree.body:
                if isinstance(node, ast.ClassDef):
                    candidates[node.name] = (node, None)
                elif (isinstance(node, ast.Assign) and len(node.targets) == 1
                      and isinstance(node.targets[0], ast.Name)):
                    candidates[node.targets[0].id] = (node, None)
                elif isinstance(node, (ast.Import, ast.ImportFrom)):
                    for alias in node.names:
                        candidates[alias.asname or alias.name.split(".")[0]] = (node, alias)
            self.contexts[path] = {
                "counts": counts, "mutated": mutated, "shadowed": shadowed,
                "assigned": assigned, "class_safe": class_confidence(tree, shadowed, assigned),
                "candidates": candidates,
                "attribute_writes": {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)
                                     and isinstance(n.ctx, (ast.Store, ast.Del))},
            }

    def binding(self, path, name, before):
        context = self.contexts[path]
        candidate = context["candidates"].get(name)
        if (candidate is None or context["counts"].get(name) != 1
                or name in context["mutated"] or "*" in context["shadowed"]):
            return None
        node, _ = candidate
        if (node.end_lineno, node.end_col_offset) >= before:
            return None
        return candidate

    def unique_module(self, path):
        name = module_name(path)
        return name if name and self.modules.get(name) == [path] else None

    def relative_target(self, path, node):
        if not node.level or not node.module:
            return None
        package = Path(path).parent
        for _ in range(node.level):
            initializer = (package / "__init__.py").as_posix()
            if initializer not in self.trees or not self.unique_module(initializer):
                return None
            if _ < node.level - 1:
                package = package.parent
        parts = node.module.split(".")
        if not all(part.isidentifier() for part in parts):
            return None
        target = package.joinpath(*parts)
        options = [target.with_suffix(".py").as_posix(), (target / "__init__.py").as_posix()]
        present = [p for p in options if p in self.selected]
        if len(present) != 1 or present[0] not in self.trees or not self.unique_module(present[0]):
            return None
        # Intermediate packages must also be selected and unambiguous.
        parent = target.parent
        while parent != package:
            initializer = (parent / "__init__.py").as_posix()
            if initializer not in self.trees or not self.unique_module(initializer):
                return None
            parent = parent.parent
        return present[0]

    def constructor(self, path, node, before):
        context = self.contexts[path]
        if isinstance(node, ast.Name) and node.id in {*COLLECTION_ARITY, "tuple"}:
            return node.id if node.id not in context["shadowed"] else None
        if self.typing_shadowed:
            return None
        if isinstance(node, ast.Name):
            binding = self.binding(path, node.id, before)
            if binding:
                declaration, alias = binding
                if (isinstance(declaration, ast.ImportFrom) and declaration.level == 0
                        and declaration.module == "typing" and alias.name in TYPING_FORMS
                        and alias.name not in context["attribute_writes"]):
                    return alias.name
        if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name):
            binding = self.binding(path, node.value.id, before)
            if binding:
                declaration, alias = binding
                if (isinstance(declaration, ast.Import) and alias.name == "typing"
                        and node.attr in TYPING_FORMS):
                    return node.attr
        return None

    def named(self, path, name, before, active, budget):
        if budget <= 0 or (path, name) in active:
            return None
        binding = self.binding(path, name, before)
        if binding is None or not self.unique_module(path):
            return None
        key = (path, name, budget)
        if key in self.cache:
            return self.cache[key]
        declaration, alias = binding
        context = self.contexts[path]
        active = active | {(path, name)}
        result = None
        if isinstance(declaration, ast.ClassDef):
            if context["class_safe"][declaration] and name not in context["assigned"]:
                result = (self.unique_module(path) + "." + name, ())
        elif isinstance(declaration, ast.Assign):
            result = self.resolve(path, declaration.value,
                                  (declaration.lineno, declaration.col_offset), active, budget - 1)
        elif isinstance(declaration, ast.ImportFrom) and declaration.level:
            target = self.relative_target(path, declaration)
            if target:
                target_binding = self.binding(target, alias.name, (float("inf"), 0))
                # Imported targets must directly declare a class or assignment alias.
                if target_binding and isinstance(target_binding[0], (ast.ClassDef, ast.Assign)):
                    result = self.named(target, alias.name, (float("inf"), 0), active, budget - 1)
        if result and not bounded_type(result):
            result = None
        self.cache[key] = result
        return result

    def resolve(self, path, node, before, active=frozenset(), budget=TYPE_DEPTH):
        context = self.contexts[path]
        if node is None or budget <= 0 or "*" in context["shadowed"]:
            return None
        if isinstance(node, ast.Name):
            if node.id in BUILTINS:
                return (node.id, ()) if node.id not in context["shadowed"] else None
            return self.named(path, node.id, before, active, budget)
        if isinstance(node, ast.Constant):
            return ("None", ()) if node.value is None else None
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
            items = [self.resolve(path, part, before, active, budget - 1) for part in (node.left, node.right)]
            return type_union(items) if all(items) else None
        if isinstance(node, ast.Subscript):
            constructor = self.constructor(path, node.value, before)
            arguments = node.slice.elts if isinstance(node.slice, ast.Tuple) else [node.slice]
            if not constructor or not arguments:
                return None
            name = {"List": "list", "Dict": "dict", "Tuple": "tuple", "Set": "set", "FrozenSet": "frozenset"}.get(constructor, constructor)
            arity = {**COLLECTION_ARITY, "Optional": 1}.get(name)
            if (arity is not None and len(arguments) != arity
                    or name == "Union" and len(arguments) < 2):
                return None
            items = [self.resolve(path, arg, before, active, budget - 1) for arg in arguments]
            if not all(items):
                return None
            if name == "Optional":
                return type_union([*items, ("None", ())])
            if name == "Union":
                return type_union(items)
            return (name, tuple(items))
        return None

    def fact(self, path, annotation):
        display = ast.unparse(annotation) if annotation else "unknown"
        resolved = self.resolve(path, annotation, (annotation.lineno, annotation.col_offset)) if annotation else None
        normalized = render_type(resolved) if resolved and bounded_type(resolved) else None
        if normalized is not None and len(normalized.encode("utf-8")) > TYPE_TEXT:
            normalized = None
        return {"display": display, "normalized": normalized if normalized is not None else display,
                "confidence": "exact" if normalized is not None else "incomplete"}


def rebound_names(tree):
    # Preserve the adapter's conservative, file-wide replacement detection.
    names = {n.id for n in ast.walk(tree)
             if isinstance(n, ast.Name) and isinstance(n.ctx, (ast.Store, ast.Del))}
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            names.update(alias.asname or alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.Attribute) and isinstance(node.ctx, (ast.Store, ast.Del)):
            names.add(node.attr)
        elif isinstance(node, ast.ExceptHandler) and node.name:
            names.add(node.name)
        elif isinstance(node, (ast.MatchAs, ast.MatchStar)) and node.name:
            names.add(node.name)
        elif isinstance(node, ast.MatchMapping) and node.rest:
            names.add(node.rest)
    return names


def class_confidence(tree, shadowed, assigned):
    """Resolve only earlier, unique module-level classes; never evaluate bases."""
    classes = {node.name: node for node in tree.body if isinstance(node, ast.ClassDef)}
    declarations = {}
    for node in ast.walk(tree):
        if isinstance(node, (*FUNCTIONS, ast.ClassDef)):
            declarations[node.name] = declarations.get(node.name, 0) + 1
    hooks = {"__init_subclass__", "__getattribute__", "__getattr__"}
    cache = {}

    def has_hooks(node):
        return any(
            isinstance(item, FUNCTIONS) and item.name in hooks
            or isinstance(item, ast.Name) and isinstance(item.ctx, (ast.Store, ast.Del)) and item.id in hooks
            for statement in node.body for item in ast.walk(statement)
        ) or bool(hooks & assigned)

    def safe(node, active=frozenset()):
        if node in cache:
            return cache[node]
        if node in active:
            return False
        if (node.decorator_list or node.keywords or getattr(node, "type_params", [])
                or "*" in assigned or node.name in assigned
                or declarations.get(node.name) != 1):
            return False
        if not node.bases:
            return True
        if classes.get(node.name) is not node or len(node.bases) != 1 or has_hooks(node):
            return False
        base = node.bases[0]
        if not isinstance(base, ast.Name):
            return False
        if base.id == "object":
            return base.id not in shadowed and base.id not in assigned
        parent = classes.get(base.id)
        if parent is None or parent.end_lineno >= node.lineno or has_hooks(parent):
            return False
        cache[node] = safe(parent, active | {node})
        return cache[node]

    # Resolve inheritance before emission. For classes without bases, preserve
    # the existing scope-aware checks in visit() and final rebinding validation.
    return {
        node: safe(node) if node.bases else not (
            node.decorator_list or node.keywords or getattr(node, "type_params", []))
        for node in ast.walk(tree) if isinstance(node, ast.ClassDef)
    }


def analyze_file(path, text):
    tree = ast.parse(text, filename=path)
    return emit_file(path, tree, TypeResolver({path: tree}))


def emit_file(path, tree, resolver):
    context = resolver.contexts[path]
    shadowed = context["shadowed"]
    assigned = context["assigned"]
    class_safe = context["class_safe"]
    symbols = {}
    definitions = {}

    def visit(body, prefix="", uncertain=False, in_class=False):
        for node in body:
            if isinstance(node, (*FUNCTIONS, ast.ClassDef)):
                name = prefix + node.name
                definitions[name] = definitions.get(name, 0) + 1
            if isinstance(node, ast.ClassDef):
                qualified = prefix + node.name
                symbols[qualified] = {
                    "id": f"python:{path}#{qualified}", "adapter": "python",
                    "language": "python", "name": node.name, "qualifiedName": qualified,
                    "kind": "class", "exported": not any(p.startswith("_") for p in qualified.split(".")),
                    "declaration": location(path, node), "signatures": [], "throws": [],
                    "confidence": "incomplete" if uncertain or not class_safe[node] else "exact",
                }
                visit(node.body, prefix + node.name + ".",
                      uncertain or not class_safe[node],
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
                    "name": arg.arg, "typeFact": resolver.fact(path, arg.annotation),
                    "optional": default is not None or rest, "rest": rest,
                    "destructured": False, "location": location(path, arg),
                } for arg, default, rest in arguments]
                returns = resolver.fact(path, node.returns)
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
    trees = {}
    selected = {path.relative_to(root).as_posix() for path in paths}
    for path in sorted(paths):
        relative = path.relative_to(root).as_posix()
        try:
            with tokenize.open(path) as stream:
                trees[relative] = ast.parse(stream.read(), filename=relative)
        except (SyntaxError, UnicodeError, OSError, ValueError, RecursionError) as error:
            diagnostics.append({"code": "PY001", "severity": "error",
                                "message": f"{relative}: {error}"})
    resolver = TypeResolver(trees, selected)
    for relative, tree in sorted(trees.items()):
        try:
            symbols = emit_file(relative, tree, resolver)
            graph["files"].append({"path": relative, "language": "python"})
            graph["symbols"].extend(symbols)
        except (ValueError, RecursionError) as error:
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
