// spec: §5.4 rule 1
// `name_range_of` and `extent_of` return `R` for tiers 0 and 1 and a null
// range otherwise (spec/api.md §5.4 rule 1): `v` written in the file
// (tier 0), `v` written in a macro argument (tier 1: the bytes of the
// argument), `v` from a macro's replacement list that is not the whole
// expansion (no range: the bytes `BODY` turn into more than `v`). Each
// reference to `v`, numbered in unit order.
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

consteval void transform() {
  std::vector<info> rs = references_to(lookup("v")[0]);
  for (unsigned i = 0; i < rs.size(); ++i) {
    at(name_range_of(rs[i]), "#" + num(i) + " name range");
    at(extent_of(rs[i]), "#" + num(i) + " extent");
  }
}
