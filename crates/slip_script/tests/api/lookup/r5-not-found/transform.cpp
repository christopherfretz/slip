// spec: §6.1 rule 5
// Not found (spec/api.md §6.1 rule 5): specialisations (explicit, partial,
// and explicit instantiations) and their members, template parameters,
// entities declared in a function body. The class template and the
// variable template are each one entity however many specialisations they
// have (rule 6).
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name :
       {"C", "C::t", "C::s", "C::p", "C::T", "v", "f", "f::local", "f::L", "f::L::k", "local", "L"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
}
