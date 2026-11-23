// spec: §6.6 rule 2
// `extent_of(reference)` (spec/api.md §6.6 rule 2): from the first token of
// the qualifier, a leading `::` included, to the last token of the name.
// Template arguments after the name are not part of it (`n::m::V`, not
// `n::m::V<int>`), template arguments inside the qualifier are
// (`n::m::V<int>::k`), and `typename` before it is not. For an unqualified
// reference it equals `name_range_of`, except that it is also non-null for
// a name that is not an identifier (`operator+`) and for one written with a
// universal-character-name (`caf\u00e9`), whose name range is null. A
// reference that is the whole expansion of an invocation (`f` from
// `ID(f)`) has the invocation as its extent and a null name range.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval void at(range r, std::string_view what) {
  if (r)
    report(r, severity::note, what);
  else
    report(severity::note, std::string(what) + ": null");
}

consteval void both(std::string_view what, info r) {
  at(extent_of(r), std::string(what) + " extent");
  at(name_range_of(r), std::string(what) + " name range");
}

consteval void transform() {
  for (std::string_view name : {"n::m::f", "n::m::V", "n::m::V::k"}) {
    std::vector<info> rs = references_to(lookup(name)[0]);
    for (unsigned i = 0; i < rs.size(); ++i) both(std::string(name) + " #" + num(i), rs[i]);
  }
  info g = lookup("g")[0];
  for (info r : references_to(parameters_of(g)[1])) both("the parameter", r);
  info call = calls_to(lookup("k2")[0])[0];
  both("operator+", callee_reference_of(arguments_as_written(call)[0]));
}
