// spec: §6.3 rule 1
// `identifier_of` (spec/api.md §6.3 rule 1): of an entity or a declaration,
// the identifier it is declared with; of a reference, the identifier
// written there. Empty for an operator, a conversion function and an
// unnamed namespace (reached as calls' callees and as a parent). Throws
// `wrong_kind` for an expression.
#include <slip/meta.h>

using namespace slip;

consteval void show(std::string_view what, info i) {
  report(severity::note, std::string(what) + ": [" + identifier_of(i) + "]");
}

consteval void transform() {
  info m = lookup("n::S::m")[0];
  show("the entity n::S::m", m);
  show("its declaration", declarations_of(m)[0]);
  show("its reference", references_to(m)[0]);
  info conversion = arguments_as_written(calls_to(lookup("k")[0])[0])[0];
  info assignment = arguments_as_written(calls_to(lookup("k2")[0])[0])[0];
  show("a conversion function", callee_of(conversion));
  show("its reference", callee_reference_of(conversion));
  show("an assignment operator", callee_of(assignment));
  show("an unnamed namespace", parent_of(lookup("n::u")[0]));
  try {
    (void)identifier_of(conversion);
    report(severity::note, "an expression: no throw");
  } catch (const error& e) {
    report(severity::note, e.code() == errc::wrong_kind ? "an expression: wrong_kind"
                                                       : "an expression: another errc");
  }
}
