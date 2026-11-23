// spec: §7.7
// "failures whose text is kept: the first 2^16, and the most recent"
// (spec/api.md §7.7); exceeding it fails nothing. The run makes 65538
// failures (`copy` of a null range, §7.1 rule 2), keeping the error of the
// 1st and of the 65537th. The 1st's text is kept; the 65537th is neither
// among the first 2^16 nor the most recent, so its `message()` is the text
// §3.5 rule 3 gives for it.
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

consteval error fail() {
  try {
    (void)copy(range{});
  } catch (const error& e) {
    return e;
  }
  throw 0;
}

consteval void transform() {
  error first = fail();
  for (unsigned i = 2; i < 65537; ++i) (void)fail();
  error kept_not = fail();
  (void)fail();
  note(std::string("the 1st: ") + errc_name(first.code()) + ", " +
       (first.message() == kept_not.message() ? "the same text" : "another text"));
  note("the 65537th: " + kept_not.message());
}
