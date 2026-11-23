// spec: §4 rule 2
// Misuse is an exception (spec/api.md §4 rule 2): `slip::error` is an
// ordinary C++ exception in constant evaluation; the transform catches it
// and continues: it asks again, records reports after the failure, and the
// run completes. A value obtained before the failure is still live.
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
  info x = lookup("x")[0];
  report(severity::note, "1 before the failure");
  try {
    (void)lookup("not a name");
    report(severity::note, "2 not reached");
  } catch (const error& e) {
    report(severity::note, "2 caught " + errc_name(e.code()));
  }
  report(severity::note, "3 after the failure, x has " + num(declarations_of(x).size()) +
                             " declaration and identifier " + identifier_of(x));
  try {
    (void)entity_of(x);
  } catch (const error& e) {
    report(severity::note, "4 caught " + errc_name(e.code()));
  }
  report(severity::note, "5 the run goes on");
}
