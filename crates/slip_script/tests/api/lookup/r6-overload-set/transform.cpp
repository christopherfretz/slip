// spec: §6.1 rule 6
// The result holds every entity so named (spec/api.md §6.1 rule 6): each
// function of an overload set is one entity, and a function template is one
// entity; in unit order of their first declarations (§6.1). The definition
// of `g(double)` is not another entity. Each line: the position in the
// result, then the entity's kind and its parameters.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  std::vector<info> gs = lookup("n::g");
  report(severity::note, "n::g: " + num(gs.size()) + " entities");
  for (unsigned i = 0; i < gs.size(); ++i) {
    std::string s = num(i) + ": " + (is_template(gs[i]) ? "template" : "function");
    for (info p : parameters_of(gs[i])) s += " " + identifier_of(p);
    report(severity::note, s);
  }
}
