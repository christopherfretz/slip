// spec: §7.2 rule 1
// The sibling spelling is for an edit of `extent_of(R)` (spec/api.md §7.2
// rule 1: "If the edit is `replace(extent_of(R), c)`"). Here each edit
// replaces `name_range_of(R)`. For `f(2)`, unqualified, the name range is
// the extent, so the rule applies. For `n::f(1)` the name range `f` is the
// extent of no reference, so "otherwise" (rule 2) holds: the fully
// qualified text replaces `f`, after the qualifier written. What that
// text means there is verification's to check (rule 5).
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
  info g = one("n::g");
  for (info r : references_to(one("n::f"))) replace(name_range_of(r), spell(g));
}
