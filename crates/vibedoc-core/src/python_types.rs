//! Bounded comparison of written Python types; no document-side name lookup.
use std::collections::BTreeSet;
use vibedoc_protocol::{Confidence, TypeFact};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Type {
    Name(String),
    Collection(String, Vec<Type>),
    Union(BTreeSet<Type>),
}

impl Type {
    fn builtin_expression(&self) -> bool {
        match self {
            Self::Name(name) => matches!(
                name.as_str(),
                "None"
                    | "bool"
                    | "str"
                    | "int"
                    | "float"
                    | "bytes"
                    | "object"
                    | "list"
                    | "dict"
                    | "tuple"
                    | "set"
                    | "frozenset"
            ),
            Self::Collection(name, arguments) => {
                let valid_arity = match name.as_str() {
                    "list" | "set" | "frozenset" => arguments.len() == 1,
                    "dict" => arguments.len() == 2,
                    "tuple" => !arguments.is_empty(),
                    _ => false,
                };
                valid_arity && arguments.iter().all(Self::builtin_expression)
            }
            Self::Union(members) => members.iter().all(Self::builtin_expression),
        }
    }
}

struct Parser<'a> {
    rest: &'a str,
    nodes: usize,
}
impl<'a> Parser<'a> {
    fn consume(&mut self, token: char) -> bool {
        self.rest = self.rest.trim_start();
        if let Some(rest) = self.rest.strip_prefix(token) {
            self.rest = rest;
            true
        } else {
            false
        }
    }
    fn expression(&mut self, depth: usize) -> Option<Type> {
        if depth > 64 || self.nodes >= 4096 {
            return None;
        }
        self.nodes += 1;
        let first = self.atom(depth + 1)?;
        let mut members = BTreeSet::new();
        let mut add = |item| match item {
            Type::Union(nested) => members.extend(nested),
            item => {
                members.insert(item);
            }
        };
        add(first);
        while self.consume('|') {
            add(self.atom(depth + 1)?);
        }
        if members.len() == 1 {
            members.into_iter().next()
        } else {
            Some(Type::Union(members))
        }
    }
    fn atom(&mut self, depth: usize) -> Option<Type> {
        if self.consume('(') {
            let inner = self.expression(depth)?;
            return self.consume(')').then_some(inner);
        }
        self.rest = self.rest.trim_start();
        let length = self
            .rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(self.rest.len());
        let name = &self.rest[..length];
        if !name.split('.').all(|part| {
            part.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        }) {
            return None;
        }
        self.rest = &self.rest[length..];
        if self.consume('[') {
            let mut arguments = vec![self.expression(depth)?];
            while self.consume(',') {
                arguments.push(self.expression(depth)?);
            }
            if !self.consume(']') {
                return None;
            }
            Some(Type::Collection(name.into(), arguments))
        } else {
            Some(Type::Name(name.into()))
        }
    }
}
fn parse(value: &str) -> Option<Type> {
    if value.len() > 16_384 {
        return None;
    }
    let mut parser = Parser {
        rest: value,
        nodes: 0,
    };
    let result = parser.expression(0)?;
    parser.rest.trim().is_empty().then_some(result)
}

/// None means unsupported documentation or source evidence, not a mismatch.
pub(crate) fn compare(documented: &str, fact: &TypeFact) -> Option<bool> {
    if fact.confidence == Confidence::Incomplete {
        return None;
    }
    let documented = parse(documented)?;
    let normalized = parse(&fact.normalized)?;
    if documented == normalized || Some(&documented) == parse(&fact.display).as_ref() {
        return Some(true);
    }
    documented.builtin_expression().then_some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fact(display: &str, normalized: &str) -> TypeFact {
        TypeFact {
            display: display.into(),
            normalized: normalized.into(),
            confidence: Confidence::Exact,
        }
    }
    #[test]
    fn source_spellings_and_builtin_expansions_verify() {
        let optional = fact("Optional[str]", "None | str");
        assert_eq!(compare("Optional[str]", &optional), Some(true));
        assert_eq!(compare("(str | None)", &optional), Some(true));
        assert_eq!(compare("int", &optional), Some(false));
        let alias = fact("Values", "list[None | str]");
        assert_eq!(compare("Values", &alias), Some(true));
        assert_eq!(compare("list[str | None]", &alias), Some(true));
        assert_eq!(compare("list[str]", &alias), Some(false));
        assert_eq!(compare("list[Optional[str]]", &alias), None);
        assert_eq!(compare("Optional[int]", &optional), None);
    }
    #[test]
    fn nominal_types_require_established_spellings() {
        let named = fact("Reply", "pkg.models.Response");
        assert_eq!(compare("Reply", &named), Some(true));
        assert_eq!(compare("pkg.models.Response", &named), Some(true));
        assert_eq!(compare("other.Response", &named), None);
        assert_eq!(compare("Response", &named), None);
        assert_eq!(compare("str", &named), Some(false));
        let union = fact("URL | str", "pkg.urls.URL | str");
        assert_eq!(compare("str | URL", &union), Some(true));
        assert_eq!(compare("pkg.urls.URL | str", &union), Some(true));
    }
    #[test]
    fn unsupported_and_incomplete_types_abstain() {
        for text in [
            "Alias",
            "typing.Any",
            "Optional[int]",
            "str ing",
            "list[int,str]",
            "int[str]",
            "tuple[int,...]",
            "'str'",
            "int()",
            "[str]",
            "int |",
            "(str, int)",
        ] {
            assert_eq!(compare(text, &fact("str", "str")), None, "{text}");
        }
        let mut incomplete = fact("Alias", "str");
        incomplete.confidence = Confidence::Incomplete;
        assert_eq!(compare("Alias", &incomplete), None);
        assert_eq!(compare("str", &incomplete), None);
        assert!(parse(&format!("{}int{}", "list[".repeat(70), "]".repeat(70))).is_none());
    }
}
