// spec: §7.2 rule 1
// At the name range of a qualified reference (`n::f`), which is the extent
// of no reference, a spelling is a sibling's identifier or is refused
// (spec/api.md §7.2 rule 1): `spell(m::g)` there throws `invalid_argument`,
// and so does a code with a spelled piece that is not exactly one piece.
// At the reference's extent the same `spell(m::g)` is fully qualified
// (rule 2). A refused request records nothing.
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
  info r = references_to(one("n::f"))[0];
  info g = one("m::g");
  note("other scope at the name: " + OUTCOME(replace(name_range_of(r), spell(g))));
  note("two pieces at the name: " +
       OUTCOME(replace(name_range_of(r), cat(spell(one("n::h")), spell(one("n::h"))))));
  note("other scope at the extent: " + OUTCOME(replace(extent_of(r), spell(g))));
}
