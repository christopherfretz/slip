// spec: §7.1 rule 3
// Two different edits of one range (spec/api.md §7.1 rule 3 (a)): the
// first argument of `f(1, 2, 3)` is replaced once by a copy of the second
// and once by a copy of the third. Reduce rejects both; the host records
// both (§7.1 rule 3 (b): "The host records the requests (§7.3 rule 3); it
// is reduce that rejects"). Sorted by pieces (§7.6): the copy of the lower
// range first.
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
  std::vector<info> args = arguments_as_written(calls_to(one("f"))[0]);
  replace(extent_of(args[0]), copy(args[2]));
  replace(extent_of(args[0]), copy(args[1]));
}
