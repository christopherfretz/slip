//! The census of what Kiln stores of a written type, list or qualifier
//! beside an expression or a declaration (Layer S, WP-156 fix round 3;
//! `spec/digest.md` §13 item 9). [`super::slots`] is total over a type
//! by construction; an expression's operands and a declaration's fields
//! are untyped words a match cannot see, so every one Kiln's role tables
//! (`kiln_ast::roles`) and schema descriptors give a type, list or
//! qualifier role has a row here, and the test below holds the two
//! together.

/// What the digest does with one stored word.
#[cfg_attr(not(test), allow(dead_code, reason = "the census is held to Kiln's tables by its test"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Disposition {
    /// Read for its operands by the named function.
    Read(&'static str),
    /// The node's own type is the written one, and is read as such.
    NodeType,
    /// A type or list Kiln derived, never written.
    Computed(&'static str),
    /// The unit's own stream walks it (expressions, declarations).
    Stream(&'static str),
}

use Disposition::{Computed, NodeType, Read, Stream};

/// One row: the owner (an `ExprKind`, `Owner.map` of a cold map, or a
/// Kiln type's path), the word (`a`, `b`, `aux`, `ty`, `value`, or a
/// field's name) and what is done with it.
#[cfg_attr(not(test), allow(dead_code, reason = "the census is held to Kiln's tables by its test"))]
pub(crate) const SLOTS: &[(&str, &str, Disposition)] = &[
    // Expressions (`kiln_ast::roles::EXPR_ROLES`).
    ("DeclRefExpr", "b", Read("holders_of: decl_ref_extra")),
    ("MemberExpr", "ty", Computed("the member's type")),
    ("DependentScopeDeclRefExpr", "a", Read("holders_of: the node's qualifier")),
    ("SubstNonTypeTemplateParmPackExpr", "b", Computed("the substituted pack")),
    ("CompoundAssignOperator", "b", Computed("the computation types")),
    ("ConditionalOperator", "b", Stream("the arms")),
    ("ChooseExpr", "b", Stream("the arms")),
    ("InitListExpr", "b", Stream("the syntactic form and the union field")),
    ("CStyleCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("CXXFunctionalCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("CXXStaticCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("CXXConstCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("CXXReinterpretCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("CXXDynamicCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("BuiltinBitCastExpr", "ty", Read("holders_of: cast_written_type")),
    ("CXXTemporaryObjectExpr", "ty", NodeType),
    ("CXXScalarValueInitExpr", "ty", NodeType),
    ("ConvertVectorExpr", "ty", NodeType),
    ("CXXUnresolvedConstructExpr", "ty", Read("holders_of: unresolved_construct_type")),
    ("CXXNewExpr", "a", Read("holders_of: new_extra allocated_ty")),
    ("CXXPseudoDestructorExpr", "b", Read("holders_of: pseudo_destructor")),
    ("CXXTypeidExpr", "a", Read("holders_of: typeid_operand")),
    ("UnaryExprOrTypeTraitExpr", "a", Read("holders_of: trait_type_arg")),
    ("TypeTraitExpr", "a", Read("holders_of: type_trait_args")),
    ("ArrayTypeTraitExpr", "a", Read("holders_of: array_trait_type")),
    ("RequiresExpr", "b", Read("holders_of: the parameters and type requirements")),
    ("ConceptSpecializationExpr", "b", Read("holders_of: the arguments as written")),
    ("VAArgExpr", "b", Read("holders_of: va_arg")),
    ("GenericSelectionExpr", "a", Read("holders_of: generic_control, generic_assocs")),
    ("CompoundLiteralExpr", "ty", Read("holders_of: compound_literal_written_type")),
    ("OffsetOfExpr", "ty", Read("holders_of: offsetof_designator, offsetof_pattern")),
    // Cold maps keyed by an expression or a declaration (`COLD_MAPS`).
    ("OperLookups.by_expr", "value", Computed("the operator lookup sets")),
    ("TemplateTables.defaults", "value", Read("head: a default type argument")),
    ("TemplateTables.pattern_mem_inits", "value", Stream("the pattern's mem-initializers")),
    ("TemplateTables.expr_targs", "value", Read("holders_of: expr_targs")),
    ("TemplateTables.expr_nns", "value", Read("holders_of: expr_nns")),
    ("TemplateTables.size_of_pack_partial", "value", Computed("a partial substitution")),
    ("TemplateTables.pseudo_dtor_first", "value", Read("holders_of: pseudo_dtor_first")),
    ("ColdExprData.member_extra", "value", Read("holders_of: decl_ref_extra")),
    ("ColdExprData.member_overload", "value", Read("holders_of: member_overload")),
    ("ColdExprData.designators", "value", Stream("the designators")),
    ("ColdExprData.array_trait", "value", Read("holders_of: array_trait_type")),
    ("ColdExprData.source_loc", "value", Computed("the values of a source location")),
    ("ColdExprData.string_maker", "value", Computed("a string's maker")),
    ("ColdExprData.use_imm", "value", Computed("the immediate invocations")),
    ("ColdExprData.cast_written", "value", Read("holders_of: cast_written_type")),
    (
        "ColdExprData.compound_literal_written",
        "value",
        Read("holders_of: compound_literal_written_type"),
    ),
    ("ColdExprData.offsetof_pattern", "value", Read("holders_of: offsetof_pattern")),
    ("ColdExprData.offsetof_layout", "value", Computed("the layout steps")),
    ("ColdExprData.offsetof_designator", "value", Read("holders_of: offsetof_designator")),
    ("ColdExprData.splice_args", "value", Stream("a splice: the walk is indeterminate")),
    ("ColdExprData.fp_round", "value", Computed("a floating-point rounding")),
    (
        "ColdDeclData.ctor_inits",
        "value",
        Stream("each initializer; a base's is its class, its id as written is not kept"),
    ),
    ("ColdDeclData.conversion_spec_target", "value", Computed("a conversion's target")),
    ("ColdDeclData.init_fp", "value", Computed("a floating-point override")),
    ("ColdDeclData.subst_failed", "value", Computed("a substitution failure")),
    // The fields of declarations' and expressions' payloads and side
    // records (Kiln's schema descriptors).
    ("kiln_ast::decl::BaseSpec", "ty", Read("bases_of")),
    (
        "kiln_ast::decl::ColdDeclData",
        "conversion_spec_target",
        Computed("a conversion's target, substituted"),
    ),
    ("kiln_ast::decl::DeclNode", "ty", Read("signature_of, function_type")),
    ("kiln_ast::decl::EnumData", "int_type", Read("underlying_of")),
    ("kiln_ast::decl::EnumData", "promotion_type", Computed("the promotion type")),
    ("kiln_ast::decl::ParamData", "original_ty", Read("param")),
    ("kiln_ast::decl::TypedefData", "underlying", Read("signature_of: a typedef")),
    ("kiln_ast::friend_form::FriendForm", "args", Read("friend_form")),
    ("kiln_ast::friend_form::FriendForm", "qualifier", Read("friend_form")),
    (
        "kiln_ast::templates::PartialSpecInfo",
        "args",
        Computed("converted; args_as_written is read"),
    ),
    ("kiln_ast::templates::PartialSpecInfo", "canonical", Computed("the canonical arguments")),
    ("kiln_ast::templates::PartialSpecInfo", "params", Read("signature_of: head")),
    (
        "kiln_ast::templates::PatternMemInitId",
        "ty",
        Stream("the constructor's stream is indeterminate (decl_class::ctor_inits)"),
    ),
    ("kiln_ast::templates::Requirement", "0", Read("holders_of: a type requirement")),
    ("kiln_ast::templates::SpecInfo", "args", Computed("converted; args_as_written is read")),
    ("kiln_ast::templates::SpecInfo", "from_args", Computed("the partial's deduced arguments")),
    ("kiln_ast::templates::SpecInfo", "print_args", Computed("the arguments for printing")),
    ("kiln_ast::templates::TemplateInfo", "params", Read("head")),
    ("kiln_ast::templates::TemplateParmInfo", "params", Read("head")),
    ("kiln_ast::templates::TemplateTables", "args_as_written", Read("signature_of")),
    ("kiln_ast::templates::TemplateTables", "expr_nns", Read("holders_of")),
    ("kiln_ast::templates::TemplateTables", "expr_targs", Read("holders_of")),
    ("kiln_ast::templates::TemplateTables", "partial_map", Computed("an index of partials")),
    ("kiln_ast::templates::TemplateTables", "pseudo_dtor_first", Read("holders_of")),
    ("kiln_ast::templates::TemplateTables", "size_of_pack_partial", Computed("a substitution")),
    (
        "kiln_ast::templates::TemplateTables",
        "specializations",
        Computed("an index of specialisations"),
    ),
    ("kiln_ast::expr::ColdExprData", "cast_written", Read("holders_of")),
    ("kiln_ast::expr::ColdExprData", "compound_literal_written", Read("holders_of")),
    ("kiln_ast::expr::ColdExprData", "offsetof_designator", Read("holders_of")),
    ("kiln_ast::expr::ColdExprData", "offsetof_pattern", Read("holders_of")),
    ("kiln_ast::expr::ColdExprData", "splice_args", Stream("a splice: the walk is indeterminate")),
    ("kiln_ast::expr::CompoundAssignExtra", "lhs_ty", Computed("the computation type")),
    ("kiln_ast::expr::CompoundAssignExtra", "result_ty", Computed("the computation type")),
    ("kiln_ast::expr::DeclRefExtra", "qualifier", Read("holders_of")),
    ("kiln_ast::expr::DeclRefExtra", "template_args", Read("holders_of")),
    (
        "kiln_ast::expr::ExprNode",
        "ty",
        Computed("an expression's type; where it is written, the kind's `ty` row"),
    ),
    ("kiln_ast::expr::NewExtra", "allocated_ty", Read("holders_of")),
    ("kiln_ast::expr::OverloadedMember", "qualifier", Read("holders_of")),
    ("kiln_ast::expr::OverloadedMember", "template_args", Read("holders_of")),
];

/// Kiln's schema descriptors, as the tables test reads them.
#[cfg(test)]
#[path = "../../tests/tables/kiln.rs"]
#[allow(dead_code, unreachable_pub)]
mod desc;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use slip_kiln::kiln::ast::roles::{COLD_MAPS, EXPR_ROLES, Role};

    use super::SLOTS;
    use super::desc;

    /// Whether `r` holds a type, a list or a qualifier, or a record whose
    /// fields the role does not describe.
    fn needs(r: &Role) -> bool {
        match r {
            Role::Type
            | Role::TypeId
            | Role::ArgList
            | Role::Nns
            | Role::Side(_)
            | Role::Record(_) => true,
            Role::Opt(x) | Role::List(x) => needs(x),
            Role::Cond { then, otherwise, .. } => needs(then) || needs(otherwise),
            Role::Bits(fs) => fs.iter().any(|f| needs(f.role)),
            _ => false,
        }
    }

    /// The Kiln types whose fields a declaration or an expression stores:
    /// their payloads and side records.
    fn decl_owner(base: &str) -> bool {
        ["kiln_ast::decl::", "kiln_ast::templates::", "kiln_ast::friend_form::", "kiln_ast::expr::"]
            .iter()
            .any(|p| base.starts_with(p))
    }

    /// Whether a descriptor field's type is a written type, list or
    /// qualifier.
    fn typed(ty: &str) -> bool {
        ["QualType", "TypeId", "ArgListId", "NnsId", "TemplateParamListId", "TypeList"]
            .iter()
            .any(|t| ty.contains(t))
    }

    /// `text` split at spaces outside `<>`, `[]` and `()` (a descriptor's
    /// items, as the tables test reads them).
    fn tokens(text: &str) -> Vec<&str> {
        let (mut out, mut depth, mut start) = (vec![], 0i32, 0);
        for (k, c) in text.char_indices() {
            match c {
                '<' | '[' | '(' => depth += 1,
                '>' | ']' | ')' => depth -= 1,
                ' ' if depth == 0 => {
                    if k > start {
                        out.push(&text[start..k]);
                    }
                    start = k + 1;
                }
                _ => {}
            }
        }
        if start < text.len() {
            out.push(&text[start..]);
        }
        out
    }

    /// A set of (owner, word).
    type Words = BTreeSet<(String, String)>;

    /// Every (owner, word) Kiln stores a written type, list or qualifier
    /// in, beside an expression or a declaration.
    fn required() -> (Words, Words) {
        let mut need = BTreeSet::new();
        let mut words = BTreeSet::new();
        for r in &EXPR_ROLES {
            let k = format!("{:?}", r.kind);
            words.insert((k.clone(), "ty".to_owned()));
            for (w, role) in [("a", &r.a), ("b", &r.b), ("aux", &r.aux)] {
                words.insert((k.clone(), w.to_owned()));
                if needs(role) {
                    need.insert((k.clone(), w.to_owned()));
                }
            }
        }
        for m in COLD_MAPS {
            let owner = format!("{}.{}", m.owner, m.map);
            let keyed = matches!(m.key, Role::Expr | Role::Decl);
            words.insert((owner.clone(), "value".to_owned()));
            if keyed && needs(&m.value) {
                need.insert((owner, "value".to_owned()));
            }
        }
        let mut ast = slip_kiln::kiln::snap::Schema::new();
        <slip_kiln::kiln::ast::Ast as slip_kiln::kiln::snap::Snap>::schema(&mut ast);
        let descs = desc::Descs::parse(&ast.hashes(), &[]).expect("Kiln's descriptors");
        for d in descs.by_name.values().filter(|d| decl_owner(&d.base)) {
            let text = ast
                .hashes()
                .into_iter()
                .find(|(t, _)| t.split_once(" = ").is_some_and(|(n, _)| n == d.name))
                .map(|(t, _)| t.to_owned())
                .unwrap_or_default();
            for tok in tokens(&text) {
                let Some((name, ty)) = tok.split_once(':') else { continue };
                if !d.fields.iter().any(|f| f == name)
                    && !d.variants.iter().any(|(_, fs)| fs.iter().any(|f| f == name))
                {
                    continue;
                }
                words.insert((d.base.clone(), name.to_owned()));
                if typed(ty) {
                    need.insert((d.base.clone(), name.to_owned()));
                }
            }
        }
        (need, words)
    }

    /// The census (`spec/digest.md` §13 item 9): every word Kiln's role
    /// tables give a type, list, qualifier or record role, and every
    /// typed field of a declaration's payload or side record, has a row of
    /// [`SLOTS`]; every row names a word that exists.
    #[test]
    fn every_stored_written_type_has_a_row() {
        let (need, words) = required();
        let rows: BTreeSet<(String, String)> =
            SLOTS.iter().map(|(o, w, _)| ((*o).to_owned(), (*w).to_owned())).collect();
        let missing: Vec<String> =
            need.difference(&rows).map(|(o, w)| format!("missing: {o} {w}")).collect();
        let stale: Vec<String> =
            rows.difference(&words).map(|(o, w)| format!("no such word: {o} {w}")).collect();
        let dup = SLOTS.len() - rows.len();
        assert!(
            missing.is_empty() && stale.is_empty() && dup == 0,
            "{} missing, {} stale, {dup} duplicated\n{}\n{}",
            missing.len(),
            stale.len(),
            missing.join("\n"),
            stale.join("\n")
        );
    }
}
