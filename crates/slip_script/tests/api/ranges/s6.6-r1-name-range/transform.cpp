// spec: §6.6 rule 1
// `name_range_of` (spec/api.md §6.6 rule 1): of a declaration, the
// identifier token that is its own name; of a reference, the identifier
// token of the name, without qualifier and without template arguments (`V`
// of `n::V<int>`, `S` of `n::S`). Null when the name is not an identifier
// (`operator+`), or is written with a universal-character-name
// (`caf\u00e9`) or a line splice (`sp\` newline `lice`). Throws
// `wrong_kind` for an entity and an expression.
#include <slip/meta.h>

using namespace slip;

consteval void at(range r, std::string_view what) {
  if (r)
    report(r, severity::note, what);
  else
    report(severity::note, std::string(what) + ": null");
}

consteval std::string outcome(info i) {
  try {
    (void)name_range_of(i);
    return "no throw";
  } catch (const error& e) {
    return e.code() == errc::wrong_kind ? "wrong_kind" : "another errc";
  }
}

consteval void transform() {
  info s = lookup("n::S")[0];
  at(name_range_of(declarations_of(s)[0]), "the declaration of n::S");
  for (info r : references_to(s)) at(name_range_of(r), "a reference to n::S");
  for (info r : references_to(lookup("n::V")[0])) at(name_range_of(r), "a reference to n::V");
  for (info r : references_to(lookup("n::splice")[0]))
    at(name_range_of(r), "a reference to n::splice");
  info g = lookup("g")[0];
  for (info r : references_to(parameters_of(g)[0])) at(name_range_of(r), "a reference to caf\\u00e9");
  info call = calls_to(lookup("k")[0])[0];
  at(name_range_of(callee_reference_of(arguments_as_written(call)[0])), "a reference to operator+");
  report(severity::note, "an entity: " + outcome(s));
  report(severity::note, "an expression: " + outcome(call));
}
