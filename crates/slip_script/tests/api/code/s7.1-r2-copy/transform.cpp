// spec: §7.1 rule 2
// `copy` (spec/api.md §7.1 rule 2): `copy(r)` throws invalid_handle for a
// null range; `copy(x)` is `copy(extent_of(x))`, so it throws
// invalid_handle when the extent is null (the call `f(1)` that `CALL`
// writes: §5.4, tokens of a replacement list). A copy of a range is a copy
// piece of that range. The empty code (`code{}`, §7.1's declaration, and
// `text("")`, rule 2) requests deletion (§7.3 rule 1): two requests with
// the same pieces (none) are one edit (§7.6 rule 1). The case of an empty
// range is not tested: version 0 has no query that returns one.
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
  note("copy(null range): " + OUTCOME(copy(range{})));
  info in_macro = calls_to(one("f"))[0];
  note(std::string("the extent of the call in CALL is ") +
       (extent_of(in_macro) ? "not null" : "null"));
  note("copy(a call whose extent is null): " + OUTCOME(copy(in_macro)));
  info call = calls_to(one("g"))[0];
  info arg = arguments_as_written(call)[0];
  replace(extent_of(callee_reference_of(call)), copy(arg));
  replace(extent_of(arg), text(""));
  replace(extent_of(arg), code{});
}
