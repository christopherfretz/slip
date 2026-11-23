// spec: §6.1 rule 3
// Anonymous unions and anonymous structs are transparent (spec/api.md §6.1
// rule 3), at any depth, in a class and at namespace scope.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"S::a", "S::b", "S::c", "g"}) {
    std::vector<info> found = lookup(name);
    report(severity::note, std::string(name) + ": " + num(found.size()) +
                               (found.size() == 1 && is_variable(found[0]) ? ", a variable" : ""));
  }
}
