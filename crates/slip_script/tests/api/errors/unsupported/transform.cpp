// spec: §4 table unsupported
// `unsupported` (spec/api.md §4, the table): the request is meaningful and
// this version cannot serve it; each case is listed where it arises: a
// component of a qualified name with `$` (§6.1 rule 1), `declarations_of`
// a parameter (§6.4 rule 3), `extent_of` a declaration (§6.6 rule 4). The
// value is 5.
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
  info f = lookup("f")[0];
  report(severity::note, "lookup(\"a$\"): " + OUTCOME(lookup("a$")));
  report(severity::note, "declarations_of(parameter): " + OUTCOME(declarations_of(parameters_of(f)[0])));
  report(severity::note, "extent_of(declaration): " + OUTCOME(extent_of(declarations_of(f)[0])));
}
