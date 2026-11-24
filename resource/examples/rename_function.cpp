// rename_function.cpp -- rename a free function, driven from C++.
//
// A complete Slip transform against <slip/meta.h> version 0
// (spec/api.md §10.2).
//
// Every function at namespace scope whose qualified name is RENAME_FROM is
// renamed to RENAME_TO, in the same namespace: the name is replaced at each
// declaration and at each reference, and the rename is declared, so that
// verification compares the program before and after under the new name.
//
// Give the names on the transform's command line:
//
//     -DRENAME_FROM='"lib::old_name"' -DRENAME_TO='"new_name"'
//
// A rename that misses a place does not compile, so a place that cannot be
// edited is an error report, not a review note.

#include <slip/meta.h>

#if !defined(__slip_lib_meta_edits) || !defined(__slip_lib_meta_identifier) || \
    !defined(__slip_lib_meta_scope_lookup)
#error "this transform needs the edits, identifier pieces and lookup in a scope of <slip/meta.h>"
#endif

#ifndef RENAME_FROM
#  define RENAME_FROM "lib::old_name"
#endif
#ifndef RENAME_TO
#  define RENAME_TO "new_name"
#endif

using namespace slip;

namespace {

constexpr std::string_view from = RENAME_FROM;
constexpr std::string_view to = RENAME_TO;

// Replaces the name written at a declaration or a reference.
consteval void rename_at(info place, code new_name) {
  range name = name_range_of(place);
  if (!is_editable(name)) {
    report(place, severity::error,
           "the rename is incomplete: the name written here cannot be edited");
    return;
  }
  replace(name, new_name);
}

}  // namespace

consteval void transform() {
  std::vector<info> functions = lookup(from);
  if (functions.empty()) return;  // not declared in this unit

  for (info f : functions)
    if (!is_function(f) || !is_namespace(parent_of(f))) {
      report(f, severity::error,
             "not renamed: the name also denotes something that is not a function at "
             "namespace scope");
      return;
    }

  // The new name, in the scope each function is declared in.
  for (info f : functions)
    for (info other : lookup(parent_of(f), to)) {
      report(other, severity::error, "not renamed: the new name is already declared here");
      return;
    }

  // The new name enters the code as an identifier, never as text.
  code new_name = identifier(to);
  for (info f : functions) {
    expect_renamed(f, to);
    for (info d : declarations_of(f)) rename_at(d, new_name);
    for (info r : references_to(f)) rename_at(r, new_name);
  }
}
