// spec: §4 table not_editable
// owned: no
// `not_editable` (spec/api.md §4, the table): `replace` on a range for
// which `is_editable` is false (no file of the subject is owned). The value
// is 4. The refused request records no edit (§4 rule 3).
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
  report(severity::note, std::string("is_editable: ") + (is_editable(r) ? "true" : "false"));
  report(severity::note, "replace: " + OUTCOME(replace(r, text("y"))));
}
