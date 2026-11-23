// spec: §4 rule 6
// `message()` is a host function and its text is for a person (spec/api.md
// §4 rule 6). The text is not specified (§2.7 rule 5), so this test only
// reports that each error has a message, that asking twice gives the same
// text (§2.7 rule 5), and that an error caught earlier still has its own
// message after later failures: two errors of different kinds have
// different texts.
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
  error first = [] consteval {
    try {
      (void)lookup("a b");
    } catch (const error& e) {
      return e;
    }
    throw 0;
  }();
  std::string a = first.message();
  error second = [] consteval {
    try {
      (void)entity_of(lookup("x")[0]);
    } catch (const error& e) {
      return e;
    }
    throw 0;
  }();
  report(severity::note, std::string("the first has a message: ") + (a.empty() ? "no" : "yes"));
  report(severity::note, std::string("the first's message is the same after the second: ") +
                             (first.message() == a ? "yes" : "no"));
  report(severity::note, std::string("the two messages differ: ") +
                             (second.message() != a ? "yes" : "no"));
}
