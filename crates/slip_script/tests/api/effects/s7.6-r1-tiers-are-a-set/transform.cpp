// spec: §7.6 rule 1
// `tiers` is no part of an edit's identity: requests that are one edit join
// their tiers by union (spec/api.md §7.6 rule 1). `h.h` is included twice;
// in the first inclusion `W` is a macro and the call `g(1)` is in its
// argument (tier 1), in the second `W` is a function and the same bytes are
// a call at tier 0. Each call is replaced by a copy of its argument: one
// edit, tiers 0 and 1.
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
  std::vector<info> calls = calls_to(one("g"));
  note("calls: " + num(calls.size()));
  for (info c : calls) replace(extent_of(c), copy(arguments_as_written(c)[0]));
}
