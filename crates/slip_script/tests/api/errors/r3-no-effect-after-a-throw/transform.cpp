// spec: §4 rule 3
// owned: no
// A function that throws has had no effect (spec/api.md §4 rule 3): a
// `report` with a severity the host does not know (`invalid_argument`,
// §7.4 rule 4), a `report` whose message is longer than 2^12 bytes
// (`limit_exceeded`, §7.7), and a `replace` of a range that is not
// editable (`not_editable`, §7.3 rule 2) record nothing. Only the reports
// that say so are in the output; there is no edit.
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


consteval void transform() {
  range r = name_range_of(declarations_of(lookup("x")[0])[0]);
  report(severity::note, "report(unknown severity): " +
                             OUTCOME(report(r, static_cast<severity>(9), "must not be recorded")));
  report(severity::note, "report(long message): " +
                             OUTCOME(report(r, severity::note, std::string(4097, 'm'))));
  report(severity::note, "replace(not editable): " + OUTCOME(replace(r, text("y"))));
}
