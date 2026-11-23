// spec: §6.7 rule 4
// `callee_of(call)`: the function called, as resolved (spec/api.md §6.7
// rule 4): the overload that overload resolution chose; for a member call,
// the function that the name denotes after lookup (`D::g` for `d.g()`,
// `B::g` for `b.g()` and for `d.B::g()`), not the one a virtual call
// reaches at run time. `callee_reference_of(call)`: the reference of rule 1,
// never null, a reference whose referent is the callee.
#include <slip/meta.h>

using namespace slip;

consteval std::string name(info e) {
  std::vector<info> f = lookup("n::f");
  if (e == f[0]) return "f(int)";
  if (e == f[1]) return "f(double)";
  if (e == lookup("n::B::g")[0]) return "B::g";
  if (e == lookup("n::D::g")[0]) return "D::g";
  return "another";
}

consteval void transform() {
  for (std::string_view q : {"n::f", "n::B::g", "n::D::g"})
    for (info e : lookup(q))
      for (info c : calls_to(e)) {
        info r = callee_reference_of(c);
        report(extent_of(c), severity::note,
               "callee " + name(callee_of(c)) +
                   (!r ? ", no reference"
                       : (is_reference(r) && referent_of(r) == callee_of(c)
                              ? ", its reference's referent"
                              : ", another referent")));
      }
}
