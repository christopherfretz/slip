// spec: §6.1 rule 3
// A function or class first declared by a friend declaration is a member of
// the innermost enclosing namespace (spec/api.md §6.1 rule 3,
// [namespace.memdef]/3), wherever the befriending class is defined: `n::Out`
// is defined at global scope and its friends are members of `n`. They are
// not members of the class, nor of the global namespace.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  for (std::string_view name :
       {"n::inf", "n::In::inf", "n::outf", "n::Hidden", "n::Out::outf", "outf", "Hidden"})
    report(severity::note, std::string(name) + ": " + num(lookup(name).size()));
}
