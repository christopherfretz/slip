// spec: §7.2 rule 1
// At the name range of a declaration `D` (spec/api.md §7.2 rule 1),
// `spell(E)` is `E`'s identifier when `parent_of(E)` is the parent of `D`'s
// entity: both declarations of `n::f`, the second written with a qualifier
// (`int n::f`), get `g`. An entity of another scope is refused with
// `invalid_argument`.
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
  for (info d : declarations_of(f))
    note("sibling: " + OUTCOME(replace(name_range_of(d), spell(one("n::g")))));
  for (info d : declarations_of(f))
    note("other scope: " + OUTCOME(replace(name_range_of(d), spell(one("m::h")))));
}
