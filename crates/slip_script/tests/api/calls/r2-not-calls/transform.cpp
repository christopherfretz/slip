// spec: §6.7 rule 2
// Not calls in version 0 (spec/api.md §6.7 rule 2): a use of an overloaded
// operator written as an operator (`a + a`); a construction (`S(a)`,
// `S{1}`); a call through a pointer (`pf(1)`) or an object with
// `operator()` (`fo(2)`): `is_call` of each is false (§6.2 rule 4), while
// `t(3)` is a call. A call whose callee is a dependent name (`f(x)` in
// `t`) is not a call, and a call inside an instantiation is not one either:
// `f(1)` in `u` is one call, written in the template, though `u` is
// instantiated twice. So `calls_to(N::f)` is that one call.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  std::vector<info> calls = calls_to(lookup("N::f")[0]);
  report(severity::note, "calls_to(N::f): " + num(calls.size()));
  for (info c : calls) report(extent_of(c), severity::note, "a call of N::f");
  for (std::string_view wrapper : {"N::k", "N::k2"})
    for (info c : calls_to(lookup(wrapper)[0])) {
      info a = arguments_as_written(c)[0];
      report(extent_of(a), severity::note, is_call(a) ? "a call" : "not a call");
    }
  report(severity::note, "calls_to(N::t): " + num(calls_to(lookup("N::t")[0]).size()));
  report(severity::note, "calls_to(N::u): " + num(calls_to(lookup("N::u")[0]).size()));
}
