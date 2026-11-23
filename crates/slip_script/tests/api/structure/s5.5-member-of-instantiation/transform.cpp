// spec: §5.5
// A member of a class template instantiation (spec/api.md §5.5):
// `referent_of` and `callee_of` give the instantiated member, which is not
// the member of the template and has no written declaration; its origin is
// the member of the template, so `references_to` and `calls_to` of the
// template's member hold the uses.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void uses(std::string_view name) {
  info e = lookup(name)[0];
  for (info r : references_to(e)) {
    info x = referent_of(r);
    report(extent_of(r), severity::note,
           "a reference to " + std::string(name) +
               (x == e ? ", the template's member itself" : ", an instantiated member") +
               (is_function(x) ? ", a function" : "") + (is_variable(x) ? ", a variable" : "") +
               ", " + num(declarations_of(x).size()) + " declarations");
  }
}

consteval void transform() {
  uses("S::g");
  uses("S::k");
  std::vector<info> calls = calls_to(lookup("S::g")[0]);
  report(severity::note, "calls_to(S::g): " + num(calls.size()));
  report(severity::note, callee_of(calls[0]) == lookup("S::g")[0]
                             ? "its callee is the template's member"
                             : "its callee is the instantiated member");
}
