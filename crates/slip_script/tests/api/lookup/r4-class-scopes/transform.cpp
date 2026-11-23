// spec: §6.1 rule 4
// Classes and enumerations are scopes (spec/api.md §6.1 rule 4): their
// members are found under the class's or the enumeration's name, nested to
// any depth. A member alias is a member, found as itself.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"n::S::m", "n::S::f", "n::S::In::k", "n::S::C::e", "n::S::T",
                                "n::S::k", "n::m"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
}
