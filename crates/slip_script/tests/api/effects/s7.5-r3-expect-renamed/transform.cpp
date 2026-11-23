// spec: §7.5 rule 3
// `expect_renamed(e, n)` records the rule that undoes the rename
// (spec/api.md §7.5 rule 3). It throws wrong_kind if `e` is not an entity;
// invalid_argument if `n` is not an identifier, is `e`'s identifier, or this
// run already declared a different name for `e`; unsupported if `e` is not
// a unit keyed by name (a parameter is part of its function's unit). Two
// entities declared renamed to one name in one scope are both recorded.
// The rule's form is `spec/verify.md` §2.1's.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval std::string errc_name(errc c) {
  switch (c) {
    case errc::invalid_handle: return "invalid_handle";
    case errc::wrong_kind: return "wrong_kind";
    case errc::invalid_argument: return "invalid_argument";
    case errc::not_editable: return "not_editable";
    case errc::unsupported: return "unsupported";
    case errc::limit_exceeded: return "limit_exceeded";
  }
  return "errc " + num(static_cast<unsigned>(c));
}

// What a call did: "returned", or the name of the errc it threw.
#define OUTCOME(...)                    \
  [&]() consteval -> std::string {      \
    try {                               \
      (void)(__VA_ARGS__);              \
      return "returned";                \
    } catch (const error& e) {          \
      return errc_name(e.code());       \
    }                                   \
  }()

consteval void note(std::string_view s) { report(severity::note, s); }

consteval info one(std::string_view name) { return lookup(name)[0]; }

consteval void transform() {
  info f = one("n::f");
  note("declaration: " + OUTCOME(expect_renamed(declarations_of(f)[0], "g")));
  note("reference: " + OUTCOME(expect_renamed(references_to(f)[0], "g")));
  note("\"1g\": " + OUTCOME(expect_renamed(f, "1g")));
  note("\"\": " + OUTCOME(expect_renamed(f, "")));
  note("\"a b\": " + OUTCOME(expect_renamed(f, "a b")));
  note("\"n::g\": " + OUTCOME(expect_renamed(f, "n::g")));
  note("its own identifier: " + OUTCOME(expect_renamed(f, "f")));
  note("parameter: " + OUTCOME(expect_renamed(parameters_of(f)[0], "q")));
  note("f to g: " + OUTCOME(expect_renamed(f, "g")));
  note("f to h after g: " + OUTCOME(expect_renamed(f, "h")));
  note("taken to g: " + OUTCOME(expect_renamed(one("n::taken"), "g")));
  note("C::m to m2: " + OUTCOME(expect_renamed(one("n::C::m"), "m2")));
}
