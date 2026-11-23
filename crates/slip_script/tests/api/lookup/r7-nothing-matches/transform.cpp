// spec: §6.1 rule 7
// A name nothing matches returns an empty sequence (spec/api.md §6.1
// rule 7, §4 rule 1): no throw, whether the first component or a later one
// fails to match, or a component names something that is not a scope.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name : {"m", "n::y", "m::x", "n::x::y"}) {
    try {
      report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
    } catch (const error&) {
      report(severity::note, std::string(name) + ": threw");
    }
  }
}
