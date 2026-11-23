// spec: §6.7 rule 1
// A call is a written postfix expression `callee ( arguments )` whose
// callee, with any parentheses around it removed, is a reference in the
// role of an expression name or a member name (spec/api.md §6.7 rule 1):
// `f(x)`, `N::f(x)`, `(f)(x)`, `a.f(x)`, `p->f(x)`, `C::f(x)`. Each call
// is reported at its extent, and its callee's reference at its extent.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"N::f", "N::C::m", "N::C::s"})
    for (info c : calls_to(lookup(name)[0])) {
      report(extent_of(c), severity::note, "a call of " + std::string(name));
      report(extent_of(callee_reference_of(c)), severity::note,
             "its callee's reference, " + std::string(name));
    }
}
