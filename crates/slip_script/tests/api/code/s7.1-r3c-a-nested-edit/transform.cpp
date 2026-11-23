// spec: §7.1 rule 3
// A nested edit must be carried (spec/api.md §7.1 rule 3 (c)). In `a`, the
// call `f(g(1), 2)` is replaced by a copy of its second argument, which
// does not carry the respelling of `g` inside the first argument (both are
// rejected by reduce, `buried`); in `b`, the call is replaced by a copy of
// its first argument, which carries the respelling. The host records all
// four requests either way (§7.3 rule 3).
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
  std::vector<info> calls = calls_to(one("f"));
  info h = one("h");
  for (info inner : calls_to(one("g"))) replace(extent_of(callee_reference_of(inner)), spell(h));
  replace(extent_of(calls[0]), copy(arguments_as_written(calls[0])[1]));
  replace(extent_of(calls[1]), copy(arguments_as_written(calls[1])[0]));
}
