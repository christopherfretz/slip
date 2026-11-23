// spec: §6.5 rule 4
// What is not yet a reference (spec/api.md §6.5 rule 4, the second row): a
// name in a template that is resolved only when the template is
// instantiated. `f(x)` and `x.m` in `t` depend on `T`; they are not
// returned, though `t` is instantiated with `n::S`, where they resolve to
// `n::f` and `n::S::m`. `t` and `S` in `y`'s initialiser are returned, and so is `m` in the
// conversion function, which is not in a template.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"n::f", "n::S::m", "n::t", "n::S"})
    for (info e : lookup(name))
      for (info r : references_to(e)) report(extent_of(r), severity::note, name);
}
