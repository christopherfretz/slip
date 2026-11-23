// spec: §6.1 rule 3
// Unnamed namespaces are transparent (spec/api.md §6.1 rule 3), at any
// depth and in the global namespace: a member of an unnamed namespace is
// found as a member of the enclosing scope.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"n::x", "n::y", "z", "::z", "x"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
}
