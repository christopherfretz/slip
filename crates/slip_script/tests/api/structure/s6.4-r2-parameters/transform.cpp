// spec: §6.4 rule 2
// `parameters_of` (spec/api.md §6.4 rule 2): the parameters, in order, of a
// function, a function template, or one declaration of either. An ellipsis
// is not a parameter. For an entity, those of its first declaration in unit
// order (`a b`, not the definition's `c d`). Throws `wrong_kind` for a
// variable and for a declaration of one.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval std::string params(info i) {
  try {
    std::vector<info> ps = parameters_of(i);
    std::string s = num(ps.size()) + ":";
    for (info p : ps) s += " [" + identifier_of(p) + "]";
    return s;
  } catch (const error& e) {
    return e.code() == errc::wrong_kind ? "wrong_kind" : "another errc";
  }
}

consteval void transform() {
  info f = lookup("f")[0];
  report(severity::note, "f: " + params(f));
  report(severity::note, "the first declaration of f: " + params(declarations_of(f)[0]));
  report(severity::note, "the second declaration of f: " + params(declarations_of(f)[1]));
  report(severity::note, "t: " + params(lookup("t")[0]));
  report(severity::note, "v: " + params(lookup("v")[0]));
  report(severity::note, "none: " + params(lookup("none")[0]));
  info g = lookup("g")[0];
  report(severity::note, "g: " + params(g));
  report(severity::note, "the declaration of g: " + params(declarations_of(g)[0]));
}
