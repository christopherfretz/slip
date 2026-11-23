// spec: §6.4 rule 4
// `entity_of` (spec/api.md §6.4 rule 4): the entity a declaration
// declares, for each declaration of a function and of a namespace. Throws
// `wrong_kind` for an entity, a reference and an expression.
#include <slip/meta.h>

using namespace slip;

consteval std::string outcome(info d, info e) {
  try {
    return entity_of(d) == e ? "the entity" : "another entity";
  } catch (const error& x) {
    return x.code() == errc::wrong_kind ? "wrong_kind" : "another errc";
  }
}

consteval void transform() {
  info f = lookup("n::f")[0];
  info n = lookup("n")[0];
  for (info d : declarations_of(f)) report(name_range_of(d), severity::note, outcome(d, f));
  for (info d : declarations_of(n)) report(name_range_of(d), severity::note, outcome(d, n));
  report(severity::note, "an entity: " + outcome(f, f));
  report(severity::note, "a reference: " + outcome(references_to(f)[0], f));
  report(severity::note, "an expression: " + outcome(calls_to(f)[0], f));
}
