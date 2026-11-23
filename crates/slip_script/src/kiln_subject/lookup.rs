//! `lookup` (`spec/api.md` §6.1 rules 2 to 7): matching by declaration,
//! over a name index per scope built on the scope's first use.
//!
//! The index of a scope holds, by identifier, every entity with a written
//! declaration whose semantic scope is that scope or a scope it contains
//! transparently, at any depth (§6.1 rule 3), so a component costs one
//! probe per scope reached. The members of a scope are found by walking
//! the member lists of its bodies (each body of a namespace, the
//! definition of a class or an enumeration, the definition of a class
//! template's pattern), through linkage specifications and `export`.
//! A namespace also gets the befriended declarations of the classes that
//! are its members, at any depth of nesting and wherever their
//! definitions are written (a friend declares a member of the innermost
//! enclosing namespace, [namespace.memdef]/3; Kiln lists it only in the
//! class). A declaration whose name is qualified (`void ns::f() {}`) is a
//! member of the scope that names it, not of the one it is written in.
//!
//! Every walk is a worklist with a set of what it has seen, so it ends on
//! any AST and is never cut short: no answer is a truncated set (landed
//! decision 18).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use slip_kiln::kiln::ast::DeclId;
use slip_kiln::kiln::ast::decl::DeclKind;
use slip_kiln::kiln::ast::names::DeclNameView;

use super::KilnSubject;
use super::entities::value_kind;

/// The name index of one scope.
#[derive(Debug, Default)]
pub(super) struct Index {
    /// Symbol index of the identifier → entity numbers, sorted, no
    /// duplicate.
    names: HashMap<u32, Vec<u32>>,
    /// A typedef name for linkage purposes ([dcl.typedef]/9) → the unnamed
    /// class it names, which is a scope under that name (landed decision 5).
    linkage: HashMap<u32, u32>,
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
            let mut named_classes: Vec<DeclId> = Vec::new();
            for &s in &scopes {
                let index = self.index(s);
                if let Some(es) = index.names.get(&sym.index()) {
                    found.extend(es.iter().map(|&n| DeclId::new(n)));
                    named_classes.extend(
                        es.iter().filter_map(|n| index.linkage.get(n)).map(|&c| DeclId::new(c)),
                    );
                }
            }
            if found.is_empty() {
                return found;
            }
            found.sort_unstable();
            found.dedup();
            scopes = found.iter().copied().filter(|&e| self.is_scope(e)).collect();
            scopes.extend(named_classes);
        }
        found.sort_by(|&a, &b| self.entity_order(a, b));
        found
    }

    /// `lookup(scope, identifier)` (§6.1, `spec/landed/api/WP-132.md`):
    /// the entities `identifier` names as members of `scope`, as the last
    /// component of a qualified name that reached `scope` names them, in
    /// unit order of their first declarations. A scope is what rule 4
    /// walks into (a namespace, the global one included, a class, a class
    /// template, an enumeration), a specialisation of a class template
    /// (the members written in its definition), or a typedef name for
    /// linkage purposes (the class it names). `None` for anything else.
    pub(super) fn lookup_in_scope(&self, scope: DeclId, identifier: &[u8]) -> Option<Vec<DeclId>> {
        let ast = self.ast();
        let scope =
            if self.is_scope(scope) || self.kind(scope) == DeclKind::ClassTemplateSpecialization {
                scope
            } else {
                // Rule 4: the typedef names its unnamed class as a scope; the
                // index of the scope that declares the typedef knows which.
                let parent = self.parent_decl(scope)?;
                let class = self.index(parent).linkage.get(&scope.index()).copied()?;
                DeclId::new(class)
            };
        let Some(sym) = std::str::from_utf8(identifier).ok().and_then(|s| ast.symbols().get(s))
        else {
            return Some(Vec::new());
        };
        let mut found: Vec<DeclId> = self
            .index(scope)
            .names
            .get(&sym.index())
            .map(|es| es.iter().map(|&n| DeclId::new(n)).collect())
            .unwrap_or_default();
        found.sort_by(|&a, &b| self.entity_order(a, b));
        Some(found)
    }

    /// The index of `scope`, built on first use.
    fn index(&self, scope: DeclId) -> Arc<Index> {
        let cached = self.scopes().get(&scope.index()).map(Arc::clone);
        cached.unwrap_or_else(|| {
            let built = Arc::new(self.build_index(scope));
            self.scopes().insert(scope.index(), Arc::clone(&built));
            built
        })
    }

    /// The index of `scope`: its own members and those of every scope it
    /// contains transparently, transitively.
    fn build_index(&self, scope: DeclId) -> Index {
        let mut index = Index::default();
        let mut todo = vec![scope];
        let mut seen: HashSet<DeclId> = HashSet::from([scope]);
        while let Some(s) = todo.pop() {
            for t in self.own_members(s, &mut index) {
                if seen.insert(t) {
                    todo.push(t);
                }
            }
        }
        for es in index.names.values_mut() {
            es.sort_unstable();
            es.dedup();
        }
        index
    }

    /// Adds to `index` the members of `scope` itself; returns the scopes it
    /// contains transparently.
    fn own_members(&self, scope: DeclId, index: &mut Index) -> Vec<DeclId> {
        let ast = self.ast();
        let namespace = matches!(self.kind(scope), DeclKind::TranslationUnit | DeclKind::Namespace);
        let bodies: Vec<DeclId> = match self.kind(scope) {
            DeclKind::TranslationUnit => vec![ast.tu],
            DeclKind::Namespace => ast.redecls_of(scope).collect(),
            _ => self.class_definition(scope).into_iter().collect(),
        };
        let mut transparent: Vec<DeclId> = Vec::new();
        // The classes that are members of `scope` (for a namespace: whose
        // friends may be members of it).
        let mut classes: Vec<DeclId> = Vec::new();
        let mut consider = |d: DeclId, classes: &mut Vec<DeclId>| {
            if !value_kind(self.kind(d)) {
                return;
            }
            let w = self.as_written(d);
            if self.scope_of(w) != Some(scope) || self.instantiated_or_implicit(w) {
                return;
            }
            let e = self.entity(w);
            if namespace && self.class_definition(e).is_some() {
                classes.push(e);
            }
            if !self.findable(w) {
                return;
            }
            if let DeclNameView::Identifier(s) = ast.names.view(ast.decls.get(w).name()) {
                index.names.entry(s.index()).or_default().push(e.index());
            }
            if let Some(td) =
                ast.decls.typedef_for_anon(w).or_else(|| ast.decls.typedef_for_anon(e))
            {
                index.linkage.insert(self.entity(td).index(), e.index());
            }
            if self.is_transparent(w) {
                transparent.push(e);
            }
        };
        let mut contexts = bodies;
        while let Some(c) = contexts.pop() {
            for m in ast.decls.members(c) {
                // A friend declaration is in a class's member list, never
                // in a namespace's, and declares no member of the class:
                // its wrapper is no value kind, so `consider` skips it.
                match self.kind(m) {
                    DeclKind::LinkageSpec | DeclKind::Export => contexts.push(m),
                    _ => consider(m, &mut classes),
                }
            }
        }
        // The friends of those classes and of the classes nested in them,
        // in each one's definition, wherever it is written.
        let mut seen: HashSet<DeclId> = classes.iter().copied().collect();
        while let Some(c) = classes.pop() {
            let Some(body) = self.class_definition(c) else { continue };
            for m in ast.decls.members(body) {
                if self.kind(m) == DeclKind::Friend {
                    if let Some(x) = self.befriended(m) {
                        consider(x, &mut Vec::new());
                    }
                    continue;
                }
                if self.instantiated_or_implicit(m) {
                    continue;
                }
                let n = self.entity(self.as_written(m));
                if self.class_definition(n).is_some() && seen.insert(n) {
                    classes.push(n);
                }
            }
        }
        transparent.retain(|&t| t != scope);
        transparent
    }

    /// The definition whose member list holds the members of class (or
    /// class template, or enumeration) entity `e`, if it has one.
    fn class_definition(&self, e: DeclId) -> Option<DeclId> {
        let ast = self.ast();
        match self.kind(e) {
            DeclKind::CXXRecord | DeclKind::ClassTemplateSpecialization | DeclKind::Enum => {
                ast.definition(e)
            }
            DeclKind::ClassTemplate => {
                ast.template_info(e).and_then(|t| t.pattern).and_then(|p| ast.definition(p))
            }
            _ => None,
        }
    }

    /// §6.1 rule 5: what lookup may find, of a written declaration as
    /// written: not a specialisation of any kind (Kiln makes a partial
    /// specialisation a `CXXRecord` or `Var` with a partial specialisation
    /// link), not a template parameter, written.
    fn findable(&self, w: DeclId) -> bool {
        let ast = self.ast();
        let specialisation = ast.spec_info(w).is_some()
            || ast.partial_spec_info(w).is_some()
            || ast.member_spec_info(w).is_some();
        !specialisation && self.is_declaration(w)
    }
}
