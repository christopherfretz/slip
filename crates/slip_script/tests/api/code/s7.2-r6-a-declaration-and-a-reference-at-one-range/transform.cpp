// spec: §7.2 rule 6
// One range that is the name of a reference and of a declaration (spec/api.md
// §7.2 rule 6): the macro argument `x` of `M(x)` is the name of the
// declaration `::x` and of the reference `o::x` in the expansion. `o::x`
// is declared renamed to `g` and `identifier("g")` is written at the
// reference's name range: the key is decided by the references at the
// range alone, so the piece takes `o::x`'s key. The declaration `::x`
// there has no obligation (spec/verify.md §10.7); the digests judge it.
#include <slip/meta.h>

using namespace slip;

consteval info one(std::string_view name) { return lookup(name)[0]; }

// Declares `e` renamed to `to` and writes `identifier(to)` at the name
// range of every reference to it.
consteval void rename(info e, std::string_view to) {
  expect_renamed(e, to);
  for (info r : references_to(e)) replace(name_range_of(r), identifier(to));
}

consteval void transform() { rename(one("o::x"), "g"); }
