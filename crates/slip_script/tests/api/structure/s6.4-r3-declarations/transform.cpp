// spec: §6.4 rule 3
// `declarations_of` (spec/api.md §6.4 rule 3): the declarations of the
// entity written in the subject unit, in unit order: every redeclaration
// and the definition, one of them in a header; for a namespace, each of its
// bodies. Empty for an entity with no written declaration (the
// specialisation `tf<int>` a call resolves to, §5.5). Throws `wrong_kind`
// for a declaration and `unsupported` for a parameter. Each declaration is
// reported at its name with its position in the answer.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval std::string outcome(info i) {
  try {
    return num(declarations_of(i).size()) + " declarations";
  } catch (const error& e) {
    return e.code() == errc::wrong_kind    ? "wrong_kind"
           : e.code() == errc::unsupported ? "unsupported"
                                           : "another errc";
  }
}

consteval void transform() {
  for (std::string_view name : {"f", "n"}) {
    std::vector<info> ds = declarations_of(lookup(name)[0]);
    for (unsigned i = 0; i < ds.size(); ++i)
      report(name_range_of(ds[i]), severity::note, std::string(name) + " #" + num(i));
  }
  info tf = lookup("tf")[0];
  report(severity::note, "the specialisation tf<int>: " + outcome(callee_of(calls_to(tf)[0])));
  report(severity::note, "a declaration: " + outcome(declarations_of(tf)[0]));
  report(severity::note, "a parameter: " + outcome(parameters_of(tf)[0]));
}
