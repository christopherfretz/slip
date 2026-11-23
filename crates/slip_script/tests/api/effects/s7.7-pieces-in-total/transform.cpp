// spec: §7.7
// "pieces, in total": 2^22 (spec/api.md §7.7), counting the pieces of every
// code value the run holds, intermediate ones too. The run makes `copy(r)`
// (1 piece), `text("x")` (1), their `cat` (2) and 14 doublings (4 to 2^15):
// 2^16 pieces in all. Each `cat(c, c)` after that holds 2^16 more, so 63
// of them hold exactly 2^22 (not exceeded) and the 64th throws
// limit_exceeded.
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
  code c = cat(copy(range(extent_of(call))), text("x"));
  for (unsigned i = 0; i < 14; ++i) c = cat(c, c);
  unsigned made = 0;
  std::string last = "returned";
  for (unsigned i = 0; i < 64 && last == "returned"; ++i) {
    last = OUTCOME(cat(c, c));
    if (last == "returned") ++made;
  }
  note(num(made) + " values of 2^16 pieces returned; then " + last);
}
