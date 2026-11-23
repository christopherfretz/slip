// spec: §5.4 rule 4
// An edit to a tier 1 range changes every use of the argument, a use by `#`
// included; the API does not expose which uses exist (spec/api.md §5.4
// rule 4). What a run can show: the callee `g` of each call `TWICE` and
// `SHOW` make is respelled once per call, and the run records one edit per
// written range, at tier 1, whatever the number of uses (two calls of
// `TWICE`, one call and one `#` of `SHOW`). That every use then changes is
// apply's and verification's, outside a script test.
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
  info h = one("h");
  std::vector<info> calls = calls_to(one("g"));
  note("calls: " + num(calls.size()));
  for (info c : calls) replace(extent_of(callee_reference_of(c)), spell(h));
}
