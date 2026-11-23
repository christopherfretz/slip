// spec: §6.1 rule 4
// A class template is a scope: the members of its primary template's
// definition (spec/api.md §6.1 rule 4). A member defined out of line is
// still one entity. A member that only an explicit specialisation declares
// is not found (rule 5).
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"C", "C::m", "C::f", "C::In::k", "C::q"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
}
