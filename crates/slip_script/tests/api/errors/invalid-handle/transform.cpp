// spec: §4 table invalid_handle
// `invalid_handle` (spec/api.md §4, the table): an `info` that is null
// where a value is required, and a `range` that is null where one is
// required or does not lie in a file of the subject unit (a range made with
// std::bit_cast from a file handle the host never issued). The value of
// the enumerator is 1, and the error has a message (§4 rule 6).
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

// What a call did: "returned", or the errc it threw, its value (§4: the
// enumerators' values are part of the API), and whether it has a message.
#define OUTCOME(...)                                                          \
  [&]() consteval -> std::string {                                            \
    try {                                                                     \
      (void)(__VA_ARGS__);                                                    \
      return "returned";                                                      \
    } catch (const error& e) {                                                \
      return errc_name(e.code()) + " = " + num(static_cast<unsigned>(e.code())) + \
             (e.message().empty() ? ", no message" : ", a message");           \
    }                                                                         \
  }()


#include <bit>

struct forged_range {
  unsigned f, lo, hi;
};

consteval void transform() {
  report(severity::note, "identifier_of(null info): " + OUTCOME(identifier_of(info())));
  report(severity::note, "source_text_of(null range): " + OUTCOME(source_text_of(range())));
  range forged = std::bit_cast<range>(forged_range{77, 0, 1});
  report(severity::note, "source_text_of(forged range): " + OUTCOME(source_text_of(forged)));
}
