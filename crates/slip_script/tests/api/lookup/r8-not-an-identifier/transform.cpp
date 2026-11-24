// spec: §6.1 rule 8
// The identifier of `lookup(scope, identifier)` is one identifier as
// spec/api.md §6.1 rule 1 takes it: a qualified name, `::`, an empty text,
// a keyword, two tokens, a digit first and text that is not UTF-8 are
// `invalid_argument`; `$` is `unsupported` (rule 1). No string is taken
// apart: `n::x` is not looked up in `n`.
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
  info g = lookup("::")[0];
  note("qualified: " + OUTCOME(lookup(g, "n::x")));
  note("global: " + OUTCOME(lookup(g, "::")));
  note("empty: " + OUTCOME(lookup(g, "")));
  note("keyword: " + OUTCOME(lookup(g, "class")));
  note("two tokens: " + OUTCOME(lookup(g, "a b")));
  note("digit: " + OUTCOME(lookup(g, "1a")));
  note("not UTF-8: " + OUTCOME(lookup(g, "\xff")));
  note("dollar: " + OUTCOME(lookup(g, "a$")));
  note("n: " + num(lookup(g, "n").size()));
}
