// spec: §6.1 rule 3
// Unscoped enumerations are transparent (spec/api.md §6.1 rule 3): an
// enumerator of an unscoped enumeration, named or not, is found as a member
// of the enclosing namespace or class. An enumeration is also a scope
// (rule 4), so `n::E::A` finds the same enumerator. A scoped enumeration is
// not transparent.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"n::A", "n::E::A", "n::B", "n::S::B", "n::C", "n::K::D", "n::D"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
  report(severity::note, lookup("n::A")[0] == lookup("n::E::A")[0] ? "n::A is n::E::A"
                                                                   : "n::A is not n::E::A");
}
