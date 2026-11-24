// spec: §7.2 rule 6
// Several references at one name range (a macro argument used twice,
// spec/api.md §5.4 rule 3, §7.2 rule 6). In `BOTH(f)` the references are
// to `a::f` and `b::f`, renamed to `g` and to `h`: neither piece written
// there has a key, since the other reference's origin is not declared
// renamed to its text. In `TWICE(f)` both references are to `a::f`: the
// piece has `a::f`'s key.
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
  rename(one("a::f"), "g");
  rename(one("b::f"), "h");
}
