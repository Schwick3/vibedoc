export const PROTOCOL_VERSION = 1;

export interface Position {
  line: number;
  column: number;
}

export interface SourceRange {
  start: Position;
  end: Position;
}

export interface SourceLocation {
  path: string;
  range: SourceRange;
}

export type Confidence = "exact" | "inferred" | "incomplete";
export type SymbolKind =
  | "module"
  | "function"
  | "class"
  | "method"
  | "interface"
  | "typeAlias"
  | "enum"
  | "variable"
  | "property"
  | "defaultExport";

export interface TypeFact {
  display: string;
  normalized: string;
  confidence: Confidence;
}

export interface Parameter {
  name: string;
  typeFact: TypeFact;
  optional: boolean;
  rest: boolean;
  destructured: boolean;
  location: SourceLocation;
}

export interface Signature {
  parameters: Parameter[];
  returnType: TypeFact;
  declaration: SourceLocation;
}

export interface ThrowFact {
  typeName: string;
  location: SourceLocation;
  confidence: Confidence;
}

export interface SourceSymbol {
  id: string;
  adapter: string;
  language: string;
  name: string;
  qualifiedName: string;
  kind: SymbolKind;
  exported: boolean;
  declaration: SourceLocation;
  signatures: Signature[];
  throws: ThrowFact[];
  confidence: Confidence;
}

export type RelationshipKind =
  | "calls"
  | "reads"
  | "writes"
  | "extends"
  | "implements"
  | "imports";

export interface Relationship {
  kind: RelationshipKind;
  from: string;
  to?: string;
  display: string;
  location: SourceLocation;
  confidence: Confidence;
}

export interface SourceFileFact {
  path: string;
  language: string;
}

export interface FactGraph {
  files: SourceFileFact[];
  symbols: SourceSymbol[];
  relationships: Relationship[];
}

export interface AdapterDiagnostic {
  code: string;
  severity: "error" | "warning" | "info";
  message: string;
  location?: SourceLocation;
}

export interface AnalyzeParams {
  workspaceRoot: string;
  projects: string[];
  sourceGlobs: string[];
}

export interface AnalyzeResult {
  graph: FactGraph;
  diagnostics: AdapterDiagnostic[];
}

