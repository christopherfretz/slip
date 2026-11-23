// spec: §6.5 rule 1
// `references_to(e)` (spec/api.md §6.5 rule 1): every reference in the
// subject unit to `e`, in unit order: the one in the header comes between
// the two of the main file that are before and after the `#include`. Each
// is reported at its extent with its position in the answer. Throws
// `wrong_kind` for a declaration.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void transform() {
  info v = lookup("N::v")[0];
  std::vector<info> rs = references_to(v);
  for (unsigned i = 0; i < rs.size(); ++i)
    report(extent_of(rs[i]), severity::note, "#" + num(i));
  try {
    (void)references_to(declarations_of(v)[0]);
    report(severity::note, "a declaration: no throw");
  } catch (const error& e) {
    report(severity::note, e.code() == errc::wrong_kind ? "a declaration: wrong_kind"
                                                       : "a declaration: another errc");
  }
}
