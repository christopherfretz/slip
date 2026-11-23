// spec: §6.7 rule 3
// `calls_to(f)` (spec/api.md §6.7 rule 3): every call in the subject unit
// whose callee's origin is `f`, in unit order: the call in the header comes
// between those before and after the `#include`, and an outer call comes
// before the call in its argument. `f` may be a function template: its
// calls of two specialisations are its calls. Throws `wrong_kind` for a
// variable, a namespace and a declaration. Each call is reported at its
// extent with its position in the answer.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval std::string outcome(info i) {
  try {
    return num(calls_to(i).size()) + " calls";
  } catch (const error& e) {
    return e.code() == errc::wrong_kind ? "wrong_kind" : "another errc";
  }
}

consteval void transform() {
  for (std::string_view name : {"n::f", "n::tf"}) {
    std::vector<info> calls = calls_to(lookup(name)[0]);
    for (unsigned i = 0; i < calls.size(); ++i)
      report(extent_of(calls[i]), severity::note, std::string(name) + " #" + num(i));
  }
  report(severity::note, "a variable: " + outcome(lookup("n::v")[0]));
  report(severity::note, "a namespace: " + outcome(lookup("n")[0]));
  report(severity::note, "a declaration: " + outcome(declarations_of(lookup("n::f")[0])[0]));
}
