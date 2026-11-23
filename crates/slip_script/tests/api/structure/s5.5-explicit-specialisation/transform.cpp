// spec: §5.5
// The origin of an explicit specialisation is its template (spec/api.md
// §5.5): a use that resolves to `template <> void tf<int>(int)`, to
// `template <> void S<char>::g()` or to `template <> int vt<int>` is a
// reference to the template or to the member of the template, and a call
// of it is a call of the template. The explicit specialisation is not the
// template, and it has a written declaration. Each use is reported at its
// extent.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void uses(std::string_view name) {
  info e = lookup(name)[0];
  for (info r : references_to(e))
    report(extent_of(r), severity::note,
           "a reference to " + std::string(name) +
               (referent_of(r) == e ? ", the template itself" : ", a specialisation") + ", " +
               num(declarations_of(referent_of(r)).size()) + " declarations");
}

consteval void transform() {
  uses("tf");
  uses("S::g");
  uses("vt");
  report(severity::note, "calls_to(tf): " + num(calls_to(lookup("tf")[0]).size()));
  report(severity::note, "calls_to(S::g): " + num(calls_to(lookup("S::g")[0]).size()));
}
