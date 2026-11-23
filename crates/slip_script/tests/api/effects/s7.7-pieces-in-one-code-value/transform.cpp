// spec: §7.7
// "pieces in one code value": 2^16 (spec/api.md §7.7). `c` starts as a copy
// and a literal (two pieces, which do not merge, §7.1 rule 6) and doubles
// with `cat(c, c)`: after 15 doublings it holds 2^16 pieces, exactly the
// limit; the 16th doubling exceeds it and throws limit_exceeded. The total
// of pieces (2^22) is not reached: about 2^17 are held.
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
  code c = cat(copy(call), "x");
  unsigned doublings = 0;
  std::string last = "returned";
  for (unsigned i = 0; i < 16 && last == "returned"; ++i) {
    last = OUTCOME(c = cat(c, c));
    if (last == "returned") ++doublings;
  }
  note(num(doublings) + " doublings returned; then " + last);
}
