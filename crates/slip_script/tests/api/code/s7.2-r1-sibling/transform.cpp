// spec: §7.2 rule 1
// The sibling spelling (spec/api.md §7.2 rule 1): `replace(extent_of(R),
// spell(E))` where `E` has an identifier, is in the scope of the referent's
// origin, was found directly, and `R` has a name range: the edit recorded
// replaces the name range of `R` with the identifier of `E`, and the
// qualifier written (`n::`) is left as written. `R` is each reference to
// `n::f`, one qualified and one not; `E` is `n::g`.
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
  for (info r : references_to(one("n::f"))) replace(extent_of(r), spell(g));
}
