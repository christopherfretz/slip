// spec: §5.4 rule 3
// In tier 1, the same `R` is the range of each piece of syntax that each
// use of the argument produced (spec/api.md §5.4 rule 3): `TWICE` uses its
// argument twice, so there are two calls `g(1)`, each with the argument's
// bytes as its extent. The name `g` is written in one place, so it is one
// reference (§5.1: a reference is one place where a name is written; §5.3
// rule 2: no duplicate), and it is the callee's reference of both calls.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  info g = lookup("g")[0];
  std::vector<info> calls = calls_to(g);
  report(severity::note, "calls: " + num(calls.size()));
  for (unsigned i = 0; i < calls.size(); ++i)
    report(extent_of(calls[i]), severity::note, "call #" + num(i));
  std::vector<info> refs = references_to(g);
  report(severity::note, "references: " + num(refs.size()));
  for (unsigned i = 0; i < refs.size(); ++i)
    report(extent_of(refs[i]), severity::note, "reference #" + num(i));
  if (calls.size() == 2)
    report(severity::note, callee_reference_of(calls[0]) == callee_reference_of(calls[1])
                               ? "the two calls have one callee reference"
                               : "the two calls have two callee references");
}
