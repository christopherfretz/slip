// spec: §6.5 rule 4
// What is not yet a reference (spec/api.md §6.5 rule 4, the first row):
// the names in a using-declaration, a using-enum-declaration and a
// using-directive, a namespace alias's target, the qualifier of a
// declarator (`int n::f`) and of a class head (`struct n::S`), an
// elaborated reference that declares (`class X;`, `friend struct n::A;`),
// an explicit specialisation's target, a concept name, a designator. Only
// the names of those contexts are left out: the template argument `n::A`
// written inside a declarator's qualifier is returned, and so is an
// elaborated type specifier that does not declare (`struct n::A* pa;`).
// The other names that are returned: the type `n::P` of `p`. The feature
// tests that will announce the rows are not defined.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"n", "n::A", "n::E", "n::f", "n::W", "n::C", "n::P", "n::P::x",
                                "n::S", "X"})
    for (info e : lookup(name))
      for (info r : references_to(e)) {
        range x = extent_of(r);
        if (x)
          report(x, severity::note, name);
        else
          report(severity::note, std::string(name) + ": a reference with no extent");
      }
#ifdef __slip_lib_meta_declarative_references
  report(severity::note, "__slip_lib_meta_declarative_references is defined");
#endif
#ifdef __slip_lib_meta_dependent_references
  report(severity::note, "__slip_lib_meta_dependent_references is defined");
#endif
#ifdef __slip_lib_meta_imported_references
  report(severity::note, "__slip_lib_meta_imported_references is defined");
#endif
}
