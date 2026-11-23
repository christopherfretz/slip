// spec: §7.4 rule 4
// `severity` is open: a value the host does not know throws
// invalid_argument (spec/api.md §7.4 rule 4), in each of the three forms
// of `report`, and records nothing (§4 rule 3).
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
  info x = one("x");
  severity unknown = static_cast<severity>(3);
  note("report(3): " + OUTCOME(report(unknown, "m")));
  note("report(info, 3): " + OUTCOME(report(x, unknown, "m")));
  note("report(range, 3): " + OUTCOME(report(name_range_of(declarations_of(x)[0]), unknown, "m")));
}
