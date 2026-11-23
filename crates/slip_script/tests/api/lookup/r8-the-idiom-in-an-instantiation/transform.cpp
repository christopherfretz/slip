// spec: §6.1 rule 8
// xfail: `spell` of a member of a class template is refused when the code is made (spec/api.md §7.1 rule 4, `unsupported`: no qualified name without template arguments), before `replace` knows the range takes the sibling rule; the ruling on WP-132 item 4 writes this idiom with `spell`, so the rule is left to the coordinator (spec/landed/api/WP-132.md, "Open")
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
