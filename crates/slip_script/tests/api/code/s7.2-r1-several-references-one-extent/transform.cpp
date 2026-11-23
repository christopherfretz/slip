// spec: §7.2 rule 1
// "If several references have that extent (§5.4 rule 3), the conditions
// must hold for each of them" (spec/api.md §7.2 rule 1). The argument `f`
// of `BOTH(f)` is used twice: in `a` it names `a::f`, in `b` it names
// `b::f`; both references have the extent `f` (tier 1). Respelling the
// reference to `a::f` as `a::g`: for the reference to `b::f` the parent of
// the referent is `b`, not `a`, so the sibling spelling does not apply and
// the text is fully qualified (rule 2).
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
  std::vector<info> ra = references_to(one("a::f"));
  std::vector<info> rb = references_to(one("b::f"));
  note("references to a::f: " + num(ra.size()) + ", to b::f: " + num(rb.size()));
  if (ra.size() == 1 && rb.size() == 1)
    note(extent_of(ra[0]) == extent_of(rb[0]) ? "one extent" : "two extents");
  for (info r : ra) replace(extent_of(r), spell(one("a::g")));
}
