// spec: §7.2 rule 2
// xfail: the text of the spelling is `::n::A` as rule 2 says, but the host names the key of the enumerator `n::A`, where spec/verify.md §6.1 names the members of an unnamed enumeration under it, `n::(anonymous enum 0)::A`
// The fully qualified spelling skips an unnamed enumeration (spec/api.md
// §7.2 rule 2): the enumerator `A` of `enum { A };` in `n` is `::n::A`. The
// key is rendered by its name (spec/api.md §11 rule 3), which for a member
// of an unnamed enumeration is under the enumeration (`spec/verify.md`
// §6.1: "its members are named under it"; the ordinal from 0, as in that
// table's `(anonymous union 0)`).
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
  replace(extent_of(arguments_as_written(calls_to(one("take"))[0])[0]), spell(one("n::A")));
}
