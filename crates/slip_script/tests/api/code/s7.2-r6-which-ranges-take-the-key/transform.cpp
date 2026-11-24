// spec: §7.2 rule 6
// Which replacements take the key (spec/api.md §7.2 rule 6), `n::f`
// declared renamed to `g` after the edits are requested. At the extent
// `n::f` of the first reference, a range larger than its name range: no
// key. At the second reference's name range, an identifier piece that is
// not the whole replacement: no key. At the third reference's name range,
// the whole replacement: the key, although the `replace` came before the
// `expect_renamed`.
#include <slip/meta.h>

using namespace slip;

consteval info one(std::string_view name) { return lookup(name)[0]; }

consteval void transform() {
  info f = one("n::f");
  std::vector<info> rs = references_to(f);
  replace(extent_of(rs[0]), identifier("g"));
  replace(name_range_of(rs[1]), cat(identifier("g"), identifier("h")));
  replace(name_range_of(rs[2]), identifier("g"));
  expect_renamed(f, "g");
}
