//! `lookup` (`spec/api.md` §6.1 rules 2 to 7): matching by declaration,
//! over a name index per scope built on the scope's first use.
//!
//! The index of a scope holds, by identifier, every entity with a written
//! declaration whose semantic scope is that scope, then the index of each
//! of its transparent members (§6.1 rule 3), so a component costs one
//! probe per scope reached. The members of a scope are found by walking
//! the member lists of its bodies (each body of a namespace, the
//! definition of a class or an enumeration, the definition of a class
//! template's pattern), through linkage specifications and `export`.
//! A namespace also gets the befriended declarations of the classes
//! written in it (a friend declares a member of the innermost enclosing
//! namespace, [namespace.memdef]/3), since those are listed in the
//! class. A declaration whose name is qualified (`void ns::f() {}`) is a
//! member of the scope that names it, not of the one it is written in.

use std::collections::HashMap;
use std::rc::Rc;

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::names::DeclNameView;

use super::KilnSubject;
use super::entities::{LIMIT, value_kind};

/// The name index of one scope.
#[derive(Debug, Default)]
pub(super) struct Index {
    /// Symbol index of the identifier → entity numbers, sorted, no
    /// duplicate.
    names: HashMap<u32, Vec<u32>>,
}

impl KilnSubject<'_> {
    /// §6.1: the entities `components` names from the global namespace, in
    /// unit order of their first declarations. No component is `::`
    /// alone, which is the global namespace.
    pub(super) fn lookup_entities(&self, components: &[&[u8]]) -> Vec<DeclId> {
        let ast = self.ast();
        if components.is_empty() {
            return vec![ast.tu];
        }
        let mut scopes = vec![ast.tu];
        let mut found: Vec<DeclId> = Vec::new();
        for c in components {
            // A name the unit never interned is declared nowhere in it.
            let Some(sym) = std::str::from_utf8(c).ok().and_then(|s| ast.symbols().get(s)) else {
                return Vec::new();
            };
            found.clear();
            for &s in &scopes {
                if let Some(es) = self.index(s).names.get(&sym.index()) {
                    found.extend(es.iter().map(|&n| DeclId::new(n)));
                }
            }
            if found.is_empty() {
                return found;
            }
            found.sort_unstable();
            found.dedup();
            scopes = found.iter().copied().filter(|&e| self.is_scope(e)).collect();
        }
        found.sort_by(|&a, &b| self.entity_order(a, b));
        found
    }

    /// The index of `scope`, built on first use.
    fn index(&self, scope: DeclId) -> Rc<Index> {
        if let Some(i) = self.scopes.borrow().get(&scope.index()) {
            return Rc::clone(i);
        }
        let built = Rc::new(self.build_index(scope, 0));
        self.scopes.borrow_mut().insert(scope.index(), Rc::clone(&built));
        built
    }

    fn build_index(&self, scope: DeclId, depth: u32) -> Index {
        let ast = self.ast();
        let namespace = matches!(self.kind(scope), DeclKind::TranslationUnit | DeclKind::Namespace);
        let bodies: Vec<DeclId> = match self.kind(scope) {
            DeclKind::TranslationUnit => vec![ast.tu],
            DeclKind::Namespace => ast.redecls_of(scope).collect(),
            DeclKind::CXXRecord | DeclKind::Enum => ast.definition(scope).into_iter().collect(),
            DeclKind::ClassTemplate => ast
                .template_info(scope)
                .and_then(|t| t.pattern)
                .and_then(|p| ast.definition(p))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        let mut names: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut transparent: Vec<DeclId> = Vec::new();
        let mut consider = |d: DeclId| {
            if !value_kind(self.kind(d)) {
                return;
            }
            let w = self.as_written(d);
            if !self.findable(w) || self.scope_of(w) != Some(scope) {
                return;
            }
            let e = self.entity(w);
            if let DeclNameView::Identifier(s) = ast.names.view(ast.decls.get(w).name()) {
                names.entry(s.index()).or_default().push(e.index());
            }
            if self.is_transparent(w) {
                transparent.push(e);
            }
        };
        // Contexts whose members are members of `scope`, and (for a
        // namespace) classes whose friends may be.
        let mut contexts = bodies;
        let mut classes: Vec<DeclId> = Vec::new();
        while let Some(c) = contexts.pop() {
            for m in ast.decls.members(c) {
                match self.kind(m) {
                    DeclKind::LinkageSpec | DeclKind::Export => contexts.push(m),
                    DeclKind::Friend => {
                        if let Some(x) = self.befriended(m) {
                            consider(x);
                        }
                    }
                    _ => {
                        consider(m);
                        if namespace && let Some(body) = self.class_body(m) {
                            classes.push(body);
                        }
                    }
                }
            }
        }
        let mut steps = 0u32;
        while let Some(c) = classes.pop() {
            steps += 1;
            if steps > LIMIT * LIMIT {
                break;
            }
            for m in ast.decls.members(c) {
                if self.kind(m) == DeclKind::Friend {
                    if let Some(x) = self.befriended(m) {
                        consider(x);
                    }
                } else if let Some(body) = self.class_body(m) {
                    classes.push(body);
                }
            }
        }
        transparent.sort_unstable();
        transparent.dedup();
        transparent.retain(|&t| t != scope);
        if depth < LIMIT {
            for t in transparent {
                let cached = self.scopes.borrow().get(&t.index()).map(Rc::clone);
                let inner = cached.unwrap_or_else(|| {
                    let built = Rc::new(self.build_index(t, depth + 1));
                    self.scopes.borrow_mut().insert(t.index(), Rc::clone(&built));
                    built
                });
                for (s, es) in &inner.names {
                    names.entry(*s).or_default().extend_from_slice(es);
                }
            }
        }
        for es in names.values_mut() {
            es.sort_unstable();
            es.dedup();
        }
        Index { names }
    }

    /// The member list holder of a class written as member `m` (its own
    /// declaration, or a class template's pattern), if it is one.
    fn class_body(&self, m: DeclId) -> Option<DeclId> {
        let ast = self.ast();
        match self.kind(m) {
            DeclKind::CXXRecord
            | DeclKind::ClassTemplateSpecialization
            | DeclKind::ClassTemplatePartialSpecialization => Some(m),
            DeclKind::ClassTemplate => ast.template_info(m).and_then(|t| t.pattern),
            _ => None,
        }
    }

    /// §6.1 rule 5: what lookup may find, of a written declaration as
    /// written: not a specialisation, not a template parameter, written.
    /// (Entities declared in a function body are never reached: the walk
    /// does not enter one.)
    fn findable(&self, w: DeclId) -> bool {
        let ast = self.ast();
        let k = self.kind(w);
        let specialisation = matches!(
            k,
            DeclKind::ClassTemplateSpecialization
                | DeclKind::ClassTemplatePartialSpecialization
                | DeclKind::VarTemplateSpecialization
                | DeclKind::VarTemplatePartialSpecialization
        ) || ast.spec_info(w).is_some()
            || ast.member_spec_info(w).is_some();
        !specialisation && self.is_declaration(w)
    }
}
