import path from "node:path";
import ts from "typescript";
import type {
  AdapterDiagnostic,
  AnalyzeParams,
  AnalyzeResult,
  Confidence,
  FactGraph,
  Parameter,
  Relationship,
  Signature,
  SourceLocation,
  SourceSymbol,
  SymbolKind,
  ThrowFact,
  TypeFact,
} from "./protocol.js";

const ADAPTER = "typescript";
const COMPILER_LIB_DIRECTORY = path.dirname(ts.getDefaultLibFilePath({}));
const SUPPORTED_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx"];

interface SymbolRecord {
  fact: SourceSymbol;
  node: ts.Node;
  checker: ts.TypeChecker;
  symbol?: ts.Symbol;
}

export function analyzeWorkspace(params: AnalyzeParams): AnalyzeResult {
  const root = path.resolve(params.workspaceRoot);
  const diagnostics: AdapterDiagnostic[] = [];
  const programs = createPrograms(params, root, diagnostics);
  const files = new Map<string, { path: string; language: string }>();
  const symbols = new Map<string, SourceSymbol>();
  const relationships: Relationship[] = [];
  const conflicts = new Set<string>();

  for (const program of programs) {
    const checker = program.getTypeChecker();
    const exportedSymbols = collectExportedSymbols(program, checker);
    const records: SymbolRecord[] = [];
    // Symbols belong to their compiler Program. Resolve each program's edges
    // before combining facts for shared files across project configurations.
    for (const sourceFile of program.getSourceFiles()) {
      const absolute = path.resolve(sourceFile.fileName);
      if (!isWorkspaceSource(absolute, root, sourceFile)) continue;
      const relative = relativePath(root, absolute);
      files.set(relative, { path: relative, language: languageFor(sourceFile.fileName) });
      collectSymbols(sourceFile, checker, root, exportedSymbols, records);
    }
    const merged = mergeSymbolRecords(records);
    const idBySymbol = new Map<ts.Symbol, string>();
    for (const record of merged) {
      if (record.symbol) idBySymbol.set(record.symbol, record.fact.id);
    }
    for (const record of merged) {
      relationships.push(...collectRelationships(record, checker, root, idBySymbol));
      const existing = symbols.get(record.fact.id);
      if (!existing) {
        symbols.set(record.fact.id, record.fact);
      } else if (JSON.stringify(existing) !== JSON.stringify(record.fact)) {
        conflicts.add(record.fact.id);
      }
    }
    // Surface resolution failures without turning Vibedoc into a full tsc run.
    const resolutionCodes = new Set([2304, 2305, 2307, 2503, 2694, 2724, 7016]);
    for (const diagnostic of program.getSemanticDiagnostics()) {
      if (resolutionCodes.has(diagnostic.code) && diagnostic.file &&
          isWorkspaceSource(path.resolve(diagnostic.file.fileName), root, diagnostic.file)) {
        diagnostics.push({ ...tsDiagnostic(diagnostic, root), severity: "warning" });
      }
    }
  }
  for (const id of conflicts) {
    const fact = symbols.get(id)!;
    fact.confidence = "incomplete";
    for (const signature of fact.signatures) {
      signature.returnType.confidence = "incomplete";
      for (const parameter of signature.parameters) parameter.typeFact.confidence = "incomplete";
    }
    for (const thrown of fact.throws) thrown.confidence = "incomplete";
    diagnostics.push({ code: "TSADAPTER004", severity: "warning",
      message: `Project configurations disagree about ${id}; its type and throw evidence is incomplete.`,
      location: fact.declaration });
  }
  for (const relationship of relationships) {
    if (conflicts.has(relationship.from)) relationship.confidence = "incomplete";
  }

  const graph: FactGraph = {
    files: [...files.values()].sort((a, b) => a.path.localeCompare(b.path)),
    symbols: [...symbols.values()].sort((a, b) => a.id.localeCompare(b.id)),
    relationships: uniqueRelationships(relationships),
  };
  return { graph, diagnostics: [...new Map(diagnostics.map((item) => [JSON.stringify(item), item])).values()]
    .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))) };
}

function createPrograms(
  params: AnalyzeParams,
  root: string,
  diagnostics: AdapterDiagnostic[],
): ts.Program[] {
  const programs: ts.Program[] = [];
  for (const requested of [...new Set(params.projects)].sort()) {
    let configPath = path.resolve(root, requested);
    if (ts.sys.directoryExists(configPath)) {
      configPath = ts.findConfigFile(configPath, ts.sys.fileExists) ?? configPath;
    }
    if (!ts.sys.fileExists(configPath)) {
      diagnostics.push({
        code: "TSADAPTER001",
        severity: "error",
        message: `Project configuration does not exist: ${configPath}`,
      });
      continue;
    }
    const read = ts.readConfigFile(configPath, ts.sys.readFile);
    if (read.error) {
      diagnostics.push(tsDiagnostic(read.error, root));
      continue;
    }
    const parsed = ts.parseJsonConfigFileContent(read.config, ts.sys, path.dirname(configPath), undefined, configPath);
    diagnostics.push(...parsed.errors.map((diagnostic) => tsDiagnostic(diagnostic, root)));
    if (parsed.errors.some((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error)) continue;
    programs.push(ts.createProgram({ rootNames: parsed.fileNames, options: parsed.options }));
  }

  if (programs.length === 0 && params.projects.length === 0 && params.sourceGlobs.length > 0) {
    const fileNames = ts.sys.readDirectory(
      root,
      SUPPORTED_EXTENSIONS,
      ["**/node_modules/**", "**/dist/**", "**/build/**"],
      params.sourceGlobs,
    );
    if (fileNames.length === 0) {
      diagnostics.push({
        code: "TSADAPTER002",
        severity: "error",
        message: "Configured source globs did not match TS, TSX, JS, or JSX files.",
      });
    } else {
      programs.push(
        ts.createProgram({
          rootNames: fileNames,
          options: {
            allowJs: true,
            checkJs: false,
            jsx: ts.JsxEmit.Preserve,
            module: ts.ModuleKind.ESNext,
            moduleResolution: ts.ModuleResolutionKind.Bundler,
            target: ts.ScriptTarget.ESNext,
          },
        }),
      );
    }
  }
  if (programs.length === 0 && diagnostics.length === 0) {
    diagnostics.push({
      code: "TSADAPTER003",
      severity: "error",
      message: "No TypeScript project configuration or fallback source globs were provided.",
    });
  }
  return programs;
}

function collectExportedSymbols(program: ts.Program, checker: ts.TypeChecker): Set<ts.Symbol> {
  const exported = new Set<ts.Symbol>();
  for (const sourceFile of program.getSourceFiles()) {
    const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
    if (!moduleSymbol) continue;
    for (const sourceSymbol of checker.getExportsOfModule(moduleSymbol)) {
      exported.add(resolveAliasWithChecker(sourceSymbol, checker));
    }
  }
  return exported;
}

function collectSymbols(
  sourceFile: ts.SourceFile,
  checker: ts.TypeChecker,
  root: string,
  exportedSymbols: Set<ts.Symbol>,
  records: SymbolRecord[],
): void {
  const relative = relativePath(root, sourceFile.fileName);
  const language = languageFor(sourceFile.fileName);

  const visit = (node: ts.Node, parents: string[], inheritedExport: boolean): void => {
    const descriptor = describeNode(node, sourceFile);
    let nextParents = parents;
    let childExport = inheritedExport;
    if (descriptor) {
      const symbol = symbolForNode(node, checker);
      const resolved = symbol ? resolveAliasWithChecker(symbol, checker) : undefined;
      const accessible = !hasNonPublicModifier(node);
      const exported =
        accessible &&
        (inheritedExport || hasExportModifier(node) || (resolved ? exportedSymbols.has(resolved) : false));
      const qualifiedName = [...parents, descriptor.name].filter(Boolean).join(".");
      const id = `${ADAPTER}:${relative}#${qualifiedName}`;
      const fact: SourceSymbol = {
        id,
        adapter: ADAPTER,
        language,
        name: descriptor.name,
        qualifiedName,
        kind: descriptor.kind,
        exported,
        declaration: location(sourceFile, node, root),
        signatures: signaturesFor(node, checker, root),
        throws: throwsFor(node, checker, root),
        confidence: confidenceForNode(node),
      };
      records.push({ fact, node, checker, ...(resolved ? { symbol: resolved } : {}) });
      if (ts.isClassDeclaration(node) || ts.isInterfaceDeclaration(node)) {
        nextParents = [...parents, descriptor.name];
        childExport = exported;
      } else if (ts.isModuleDeclaration(node)) {
        nextParents = [...parents, descriptor.name];
        // Namespace members must be explicitly exported; class members inherit visibility.
        childExport = false;
      }
      if (
        ts.isFunctionDeclaration(node) ||
        ts.isMethodDeclaration(node) ||
        ts.isMethodSignature(node) ||
        ts.isPropertyDeclaration(node) ||
        ts.isPropertySignature(node) ||
        ts.isVariableDeclaration(node)
      ) {
        return;
      }
    }
    ts.forEachChild(node, (child) => visit(child, nextParents, childExport));
  };
  visit(sourceFile, [], false);
}

function describeNode(node: ts.Node, sourceFile: ts.SourceFile): { name: string; kind: SymbolKind } | undefined {
  if (ts.isModuleDeclaration(node)) return { name: node.name.text, kind: "module" };
  if (ts.isFunctionDeclaration(node)) {
    if (node.name) return { name: node.name.text, kind: "function" };
    if (hasDefaultModifier(node)) return { name: "default", kind: "defaultExport" };
  }
  if (ts.isClassDeclaration(node)) {
    if (node.name) return { name: node.name.text, kind: "class" };
    if (hasDefaultModifier(node)) return { name: "default", kind: "defaultExport" };
  }
  if (ts.isMethodDeclaration(node) || ts.isMethodSignature(node)) {
    return { name: propertyName(node.name, sourceFile), kind: "method" };
  }
  if (ts.isPropertyDeclaration(node) || ts.isPropertySignature(node)) {
    return { name: propertyName(node.name, sourceFile), kind: "property" };
  }
  if (ts.isInterfaceDeclaration(node)) return { name: node.name.text, kind: "interface" };
  if (ts.isTypeAliasDeclaration(node)) return { name: node.name.text, kind: "typeAlias" };
  if (ts.isEnumDeclaration(node)) return { name: node.name.text, kind: "enum" };
  if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name)) {
    const functionValue = node.initializer && (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer));
    return { name: node.name.text, kind: functionValue ? "function" : "variable" };
  }
  return undefined;
}

function signaturesFor(node: ts.Node, checker: ts.TypeChecker, root: string): Signature[] {
  const declaration = callableDeclaration(node);
  if (!declaration) return [];
  const signature = checker.getSignatureFromDeclaration(declaration);
  if (!signature) return [];
  const sourceFile = node.getSourceFile();
  const parameters: Parameter[] = declaration.parameters.map((parameter) => {
    const type = checker.getTypeAtLocation(parameter);
    const destructured = !ts.isIdentifier(parameter.name);
    return {
      name: parameter.name.getText(sourceFile),
      typeFact: typeFact(type, checker, parameter, Boolean(parameter.type)),
      optional: Boolean(parameter.questionToken || parameter.initializer),
      rest: Boolean(parameter.dotDotDotToken),
      destructured,
      location: location(sourceFile, parameter, root),
    };
  });
  const returnType = checker.getReturnTypeOfSignature(signature);
  const explicitReturn = "type" in declaration && Boolean(declaration.type);
  return [
    {
      parameters,
      returnType: typeFact(returnType, checker, declaration, explicitReturn),
      declaration: location(sourceFile, declaration, root),
    },
  ];
}

function callableDeclaration(node: ts.Node): ts.SignatureDeclaration | undefined {
  if (
    ts.isFunctionDeclaration(node) ||
    ts.isMethodDeclaration(node) ||
    ts.isMethodSignature(node)
  ) {
    return node;
  }
  if (
    ts.isVariableDeclaration(node) &&
    node.initializer &&
    (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer))
  ) {
    return node.initializer;
  }
  return undefined;
}

function throwsFor(node: ts.Node, checker: ts.TypeChecker, root: string): ThrowFact[] {
  const callable = callableDeclaration(node);
  const body = callable && "body" in callable ? callable.body : undefined;
  if (!body) return [];
  const output: ThrowFact[] = [];
  const visit = (child: ts.Node): void => {
    if (child !== body && ts.isFunctionLike(child)) return;
    if (ts.isThrowStatement(child) && child.expression) {
      const expression = child.expression;
      let typeName: string;
      let confidence: Confidence = "inferred";
      const thrownType = checker.getTypeAtLocation(expression);
      if (ts.isNewExpression(expression)) {
        typeName = expression.expression.getText();
        confidence = "exact";
      } else {
        typeName = checker.typeToString(thrownType, expression);
      }
      if (isIncompleteType(thrownType, checker)) confidence = "incomplete";
      output.push({
        typeName,
        location: location(node.getSourceFile(), child, root),
        confidence,
      });
    }
    ts.forEachChild(child, visit);
  };
  visit(body);
  return output;
}

function collectRelationships(
  record: SymbolRecord,
  checker: ts.TypeChecker,
  root: string,
  idBySymbol: Map<ts.Symbol, string>,
): Relationship[] {
  const callable = callableDeclaration(record.node);
  const body = callable && "body" in callable ? callable.body : undefined;
  if (!body) return heritageRelationships(record, checker, root, idBySymbol);
  const output: Relationship[] = [];
  const visit = (node: ts.Node): void => {
    if (node !== body && ts.isFunctionLike(node)) return;
    if (ts.isCallExpression(node)) {
      const symbol = checker.getSymbolAtLocation(node.expression);
      const resolved = symbol ? resolveAliasWithChecker(symbol, checker) : undefined;
      const target = resolved ? idBySymbol.get(resolved) : undefined;
      output.push({
        kind: "calls",
        from: record.fact.id,
        ...(target ? { to: target } : {}),
        display: node.expression.getText(),
        location: location(node.getSourceFile(), node, root),
        confidence: resolved ? "exact" : "inferred",
      });
    }
    if (ts.isBinaryExpression(node) && isAssignment(node.operatorToken.kind)) {
      const symbol = checker.getSymbolAtLocation(node.left);
      const resolved = symbol ? resolveAliasWithChecker(symbol, checker) : undefined;
      const target = resolved ? idBySymbol.get(resolved) : undefined;
      output.push({
        kind: "writes",
        from: record.fact.id,
        ...(target ? { to: target } : {}),
        display: node.left.getText(),
        location: location(node.getSourceFile(), node.left, root),
        confidence: resolved ? "exact" : "inferred",
      });
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
  return output;
}

function heritageRelationships(
  record: SymbolRecord,
  checker: ts.TypeChecker,
  root: string,
  idBySymbol: Map<ts.Symbol, string>,
): Relationship[] {
  if (!ts.isClassDeclaration(record.node) && !ts.isInterfaceDeclaration(record.node)) return [];
  const output: Relationship[] = [];
  for (const clause of record.node.heritageClauses ?? []) {
    for (const type of clause.types) {
      const symbol = checker.getSymbolAtLocation(type.expression);
      const resolved = symbol ? resolveAliasWithChecker(symbol, checker) : undefined;
      const target = resolved ? idBySymbol.get(resolved) : undefined;
      output.push({
        kind: clause.token === ts.SyntaxKind.ExtendsKeyword ? "extends" : "implements",
        from: record.fact.id,
        ...(target ? { to: target } : {}),
        display: type.expression.getText(),
        location: location(record.node.getSourceFile(), type, root),
        confidence: resolved ? "exact" : "inferred",
      });
    }
  }
  return output;
}

function typeFact(type: ts.Type, checker: ts.TypeChecker, node: ts.Node, explicit: boolean): TypeFact {
  const display = checker.typeToString(type, node, ts.TypeFormatFlags.NoTruncation);
  const incomplete = isIncompleteType(type, checker);
  return {
    display,
    normalized: normalizeType(display),
    confidence: incomplete ? "incomplete" : explicit ? "exact" : "inferred",
  };
}

// Error types can retain an unresolved alias's display name while carrying Any.
// Inspect compiler flags and nested type arguments instead of formatted strings.
function isIncompleteType(type: ts.Type, checker: ts.TypeChecker, seen = new Set<ts.Type>()): boolean {
  if (type.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)) return true;
  if (seen.has(type)) return false;
  // Bound expansion of recursive generic structures; exhausted evidence is unknown.
  if (seen.size >= 256) return true;
  seen.add(type);
  if (type.isUnionOrIntersection() && type.types.some((part) => isIncompleteType(part, checker, seen))) {
    return true;
  }
  const argumentsToCheck = [...(type.aliasTypeArguments ?? [])];
  if (type.flags & ts.TypeFlags.Object) {
    const object = type as ts.ObjectType;
    if (object.objectFlags & ts.ObjectFlags.Reference) {
      argumentsToCheck.push(...checker.getTypeArguments(type as ts.TypeReference));
    }
  }
  if (argumentsToCheck.some((argument) => isIncompleteType(argument, checker, seen))) return true;
  if (type.flags & ts.TypeFlags.Object) {
    for (const property of checker.getPropertiesOfType(type)) {
      const declaration = property.valueDeclaration ?? property.declarations?.[0];
      // Standard-library internals do not determine a user's declared shape.
      if (!declaration || path.dirname(declaration.getSourceFile().fileName) === COMPILER_LIB_DIRECTORY) continue;
      if (isIncompleteType(checker.getTypeOfSymbolAtLocation(property, declaration), checker, seen)) return true;
    }
    for (const kind of [ts.SignatureKind.Call, ts.SignatureKind.Construct]) {
      for (const signature of checker.getSignaturesOfType(type, kind)) {
        const declaration = signature.getDeclaration();
        if (!declaration || path.dirname(declaration.getSourceFile().fileName) === COMPILER_LIB_DIRECTORY) continue;
        if (isIncompleteType(checker.getReturnTypeOfSignature(signature), checker, seen)) return true;
        for (const parameter of signature.parameters) {
          if (isIncompleteType(checker.getTypeOfSymbolAtLocation(parameter, declaration), checker, seen)) return true;
        }
      }
    }
    for (const index of checker.getIndexInfosOfType(type)) {
      if (isIncompleteType(index.type, checker, seen)) return true;
    }
  }
  return false;
}

function confidenceForNode(node: ts.Node): Confidence {
  if (ts.isVariableDeclaration(node) && !node.type) return "inferred";
  return "exact";
}

function symbolForNode(node: ts.Node, checker: ts.TypeChecker): ts.Symbol | undefined {
  const named = node as ts.NamedDeclaration;
  if (named.name) {
    return checker.getSymbolAtLocation(named.name);
  }
  return checker.getSymbolAtLocation(node);
}

function resolveAliasWithChecker(symbol: ts.Symbol, checker: ts.TypeChecker): ts.Symbol {
  return symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol;
}

function hasExportModifier(node: ts.Node): boolean {
  return Boolean(ts.getCombinedModifierFlags(node as ts.Declaration) & ts.ModifierFlags.Export);
}

function hasDefaultModifier(node: ts.Node): boolean {
  return Boolean(ts.getCombinedModifierFlags(node as ts.Declaration) & ts.ModifierFlags.Default);
}

function hasNonPublicModifier(node: ts.Node): boolean {
  const flags = ts.getCombinedModifierFlags(node as ts.Declaration);
  return Boolean(flags & (ts.ModifierFlags.Private | ts.ModifierFlags.Protected));
}

function propertyName(name: ts.PropertyName, sourceFile: ts.SourceFile): string {
  return ts.isIdentifier(name) || ts.isPrivateIdentifier(name) || ts.isStringLiteral(name)
    ? name.text
    : name.getText(sourceFile);
}

function location(sourceFile: ts.SourceFile, node: ts.Node, root: string): SourceLocation {
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  const end = sourceFile.getLineAndCharacterOfPosition(node.getEnd());
  return {
    path: relativePath(root, sourceFile.fileName),
    range: {
      start: { line: start.line + 1, column: start.character + 1 },
      end: { line: end.line + 1, column: end.character + 1 },
    },
  };
}

function tsDiagnostic(diagnostic: ts.Diagnostic, root: string): AdapterDiagnostic {
  const message = ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n");
  if (!diagnostic.file || diagnostic.start === undefined) {
    return {
      code: `TS${diagnostic.code}`,
      severity: diagnostic.category === ts.DiagnosticCategory.Error ? "error" : "warning",
      message,
    };
  }
  const start = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start);
  const end = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start + (diagnostic.length ?? 0));
  return {
    code: `TS${diagnostic.code}`,
    severity: diagnostic.category === ts.DiagnosticCategory.Error ? "error" : "warning",
    message,
    location: {
      path: relativePath(root, diagnostic.file.fileName),
      range: {
        start: { line: start.line + 1, column: start.character + 1 },
        end: { line: end.line + 1, column: end.character + 1 },
      },
    },
  };
}

function isWorkspaceSource(absolute: string, root: string, sourceFile: ts.SourceFile): boolean {
  const relative = path.relative(root, absolute);
  return (
    !relative.startsWith("..") &&
    !path.isAbsolute(relative) &&
    !sourceFile.isDeclarationFile &&
    !relative.split(path.sep).includes("node_modules") &&
    SUPPORTED_EXTENSIONS.includes(path.extname(absolute).toLowerCase())
  );
}

function languageFor(file: string): string {
  switch (path.extname(file).toLowerCase()) {
    case ".tsx":
      return "tsx";
    case ".js":
      return "javascript";
    case ".jsx":
      return "jsx";
    default:
      return "typescript";
  }
}

function relativePath(root: string, file: string): string {
  return path.relative(root, path.resolve(file)).split(path.sep).join("/");
}

function normalizeType(value: string): string {
  return value.replace(/\s+/g, "");
}

function isAssignment(kind: ts.SyntaxKind): boolean {
  return kind >= ts.SyntaxKind.FirstAssignment && kind <= ts.SyntaxKind.LastAssignment;
}

function uniqueRelationships(relationships: Relationship[]): Relationship[] {
  const seen = new Set<string>();
  return relationships
    .filter((relationship) => {
      const key = `${relationship.kind}|${relationship.from}|${relationship.to ?? ""}|${relationship.display}|${relationship.location.path}|${relationship.location.range.start.line}|${relationship.location.range.start.column}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .sort((a, b) => {
      const left = `${a.from}|${a.kind}|${a.location.path}|${a.location.range.start.line}|${a.location.range.start.column}`;
      const right = `${b.from}|${b.kind}|${b.location.path}|${b.location.range.start.line}|${b.location.range.start.column}`;
      return left.localeCompare(right);
    });
}

function mergeSymbolRecords(records: SymbolRecord[]): SymbolRecord[] {
  const merged = new Map<string, SymbolRecord>();
  for (const record of records) {
    const existing = merged.get(record.fact.id);
    if (!existing) {
      merged.set(record.fact.id, record);
      continue;
    }
    existing.fact.signatures.push(...record.fact.signatures);
    existing.fact.throws.push(...record.fact.throws);
    existing.fact.exported ||= record.fact.exported;
    const existingCallable = callableDeclaration(existing.node);
    const candidateCallable = callableDeclaration(record.node);
    const existingHasBody = Boolean(existingCallable && "body" in existingCallable && existingCallable.body);
    const candidateHasBody = Boolean(candidateCallable && "body" in candidateCallable && candidateCallable.body);
    if (!existingHasBody && candidateHasBody) {
      existing.node = record.node;
      existing.checker = record.checker;
    }
  }
  return [...merged.values()];
}
