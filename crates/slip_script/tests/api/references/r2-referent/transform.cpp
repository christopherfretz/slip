// spec: §6.5 rule 2
// `referent_of(r)` (spec/api.md §6.5 rule 2): the entity the reference
// denotes, as resolved, never null: overload resolution chose `f(int)` for
// two of the three names and `f(double)` for one, and `references_to` of
// each overload holds its own. Throws `wrong_kind` for an entity, a
// declaration and an expression.
#include <slip/meta.h>

using namespace slip;

consteval std::string outcome(info i) {
  try {
    (void)referent_of(i);
    return "no throw";
  } catch (const error& e) {
    return e.code() == errc::wrong_kind ? "wrong_kind" : "another errc";
  }
}

consteval void transform() {
  std::vector<info> fs = lookup("f");
  for (unsigned i = 0; i < fs.size(); ++i)
    for (info r : references_to(fs[i])) {
      info x = referent_of(r);
      report(extent_of(r), severity::note,
             !x ? "null" : (x == fs[0] ? "f(int)" : (x == fs[1] ? "f(double)" : "another")));
    }
  report(severity::note, "an entity: " + outcome(fs[0]));
  report(severity::note, "a declaration: " + outcome(declarations_of(fs[0])[0]));
  report(severity::note, "an expression: " + outcome(calls_to(fs[0])[0]));
}
