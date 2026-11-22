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

#if !defined(__slip_lib_meta_edits)
#error "this transform needs the edits of <slip/meta.h>"
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

// The qualified name the function will have: `from` with its last component
// replaced by `to`.
consteval std::string renamed() {
  std::string s(from);
  auto at = s.rfind("::");
  s.resize(at == std::string::npos ? 0 : at + 2);
  s += to;
  return s;
}

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

  for (info other : lookup(renamed())) {
    report(other, severity::error, "not renamed: the new name is already declared here");
    return;
  }

  code new_name = text(to);
  for (info f : functions) {
    expect_renamed(f, to);
    for (info d : declarations_of(f)) rename_at(d, new_name);
    for (info r : references_to(f)) rename_at(r, new_name);
  }
}
