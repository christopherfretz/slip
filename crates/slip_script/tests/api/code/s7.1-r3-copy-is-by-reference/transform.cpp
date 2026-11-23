// spec: §7.1 rule 3
// Copy is by reference, not by value (spec/api.md §7.1 rule 3): the two
// arguments of `f(g(1), 2)` are exchanged with copies, and the callee of
// `g(1)`, which lies properly inside the first argument, is respelled. The
// run records each copy as a copy piece of its range (§7.6 `Piece`), not
// as the bytes the range held when it was copied, so the respelling inside
// the copied range is not lost to the exchange. The host records all three
// requests (§7.3 rule 3); composing them is reduce's and apply's.
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
  info call = calls_to(one("f"))[0];
  std::vector<info> args = arguments_as_written(call);
  replace(extent_of(args[0]), copy(args[1]));
  replace(extent_of(args[1]), copy(args[0]));
  info inner = calls_to(one("g"))[0];
  replace(extent_of(callee_reference_of(inner)), spell(one("h")));
}
