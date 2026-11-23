// spec: §6.1 rule 8
// `lookup(scope, identifier)` in an enumeration (spec/api.md §6.1 rule 8):
// its enumerators, scoped or not; the enumerator of an unscoped
// enumeration is also a member of the scope around it (rule 3).
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
  note("C::a: " + num(lookup(one("C"), "a").size()));
  note("C::u1: " + num(lookup(one("C"), "u1").size()));
  note("U::u1: " + num(lookup(one("U"), "u1").size()));
  note("::u1: " + num(lookup(lookup("::")[0], "u1").size()));
  note("::a: " + num(lookup(lookup("::")[0], "a").size()));
}
