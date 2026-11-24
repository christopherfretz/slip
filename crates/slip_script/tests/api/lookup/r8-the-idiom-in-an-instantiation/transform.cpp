// spec: §6.1 rule 8
// The idiom `lookup(parent_of(referent_of(r)), "g")` at a call of a member
// of an instantiation (spec/api.md §6.1 rule 8): `w.f()` with `W<int> w`,
// `i.f()` with `O<int>::I i` (a member class of a class template) and
// `x.f()` with `X<char> x` (an explicit instantiation definition). The
// scope is the instantiation; its members are those written in the
// definition it was instantiated from, so `g` is the template's `g` and
// `spell` of it at the call's name range is a sibling of the referent's
// origin: the identifier `g` (§7.2 rule 1). No string is built.
#include <slip/meta.h>

using namespace slip;

consteval info one(std::string_view name) { return lookup(name)[0]; }

consteval void transform() {
  for (std::string_view f : {"W::f", "O::I::f", "X::f"}) {
    for (info r : references_to(one(f))) {
      std::vector<info> g = lookup(parent_of(referent_of(r)), "g");
      if (g.size() != 1) {
        report(r, severity::error, "no g in the scope of the call");
        continue;
      }
      replace(name_range_of(r), spell(g[0]));
    }
  }
}
