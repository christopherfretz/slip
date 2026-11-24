// spec: §7.2 rule 6
// As `s7.2-r6-a-declaration-and-a-reference-at-one-range`, but the
// declaration at the range is a redeclaration of the renamed entity
// itself: the macro argument `x` of `M(x)` is the name of a redeclaration
// of `o::x` and of the reference `o::x`. Writing `identifier("g")` at
// every declaration's name and at every reference's name range requests
// the same edit twice at that range, which is one edit (§7.6 rule 1). The
// key is decided by the reference named there (spec/api.md §7.2 rule 6),
// whichever request wrote it: `o::x`'s. That is right: the obligation
// asserts that the name written there denotes the renamed `o::x` in the
// post-image, which `o::g(1)` does; the redeclaration, no reference, is
// judged by the digests under the rule. The first declaration's name
// (line 1) names no reference: no key.
#include <slip/meta.h>

using namespace slip;

consteval info one(std::string_view name) { return lookup(name)[0]; }

// Declares `e` renamed to `to` and writes `identifier(to)` at the name
// range of every reference to it.
consteval void rename(info e, std::string_view to) {
  expect_renamed(e, to);
  for (info r : references_to(e)) replace(name_range_of(r), identifier(to));
}

consteval void transform() {
  info x = one("o::x");
  for (info d : declarations_of(x)) replace(name_range_of(d), identifier("g"));
  rename(x, "g");
}
