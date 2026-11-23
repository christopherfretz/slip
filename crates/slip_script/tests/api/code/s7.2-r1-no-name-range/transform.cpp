// spec: §7.2 rule 1
// The fourth condition of the sibling spelling (spec/api.md §7.2 rule 1):
// the name `f` of `n::NAME(1)` comes from the replacement list of `NAME`,
// so its name range is null (§5.4), while its extent `n::NAME` is tier 0
// (§5.4 rule 2: it contains a whole invocation). The rule does not apply,
// and the fully qualified text replaces the extent.
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
  info r = callee_reference_of(calls_to(one("n::f"))[0]);
  note(std::string("name range: ") + (name_range_of(r) ? "not null" : "null"));
  replace(extent_of(r), spell(one("n::g")));
}
