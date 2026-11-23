// spec: §7.5 rule 1
// `expect_changed(x, why)` records the key of the digest unit that holds
// `x`, with the extent of `x` (spec/api.md §7.5 rule 1): the function `f`
// for the call `g(1)` in its body and, answered on its own, for the
// callee's reference; the function `h` for the reference in a member of a
// local class (a declaration at block scope is contained by the function);
// the class `D` for the name `B` in its base specifier. No unit here has a
// derived unit (`spec/digest.md` §2.2). The order of `changed` is by key
// (§7.6), whose digests the text does not give, so the order of two keys
// is the host's.
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
  info g = one("g");
  std::vector<info> calls = calls_to(g);
  expect_changed(calls[0], "call");
  expect_changed(callee_reference_of(calls[0]), "callee");
  expect_changed(callee_reference_of(calls[1]), "local class");
  expect_changed(references_to(one("B"))[0], "base");
}
