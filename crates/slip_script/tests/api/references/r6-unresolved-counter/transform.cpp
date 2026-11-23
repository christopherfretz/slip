// spec: §6.5 rule 6
// counters: api.unresolved_spelling_matches
// The count of unresolved names spelled like an entity given to
// `references_to` or `calls_to` (spec/api.md §6.5 rule 6): the identifiers
// of the entities given to a call that succeeded form the set {f, m}; the
// unresolved names of the unit with those identifiers are the two `f` and
// the `m` in `t`: 3. A name is counted once however many entities of the
// set share its identifier (the two overloads of `f`) and however often
// the operations were called; `g` was not given, so its name is not
// counted. A call that failed adds nothing: `calls_to(n::S::k)` is
// `wrong_kind`, so the `k` in `t` is not counted.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (info f : lookup("n::f")) {
    (void)references_to(f);
    (void)calls_to(f);
    (void)references_to(f);
  }
  (void)references_to(lookup("n::S::m")[0]);
  try {
    (void)calls_to(lookup("n::S::k")[0]);  // wrong_kind: a variable
  } catch (const error&) {
  }
}
