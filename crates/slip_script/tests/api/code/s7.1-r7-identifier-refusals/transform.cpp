// spec: §7.1 rule 7
// `identifier(name)` takes one identifier as spec/api.md §6.1 rule 1 does:
// a keyword, an alternative token, two tokens, an empty text, a qualified
// name, a digit first and text that is not UTF-8 are `invalid_argument`;
// `$` and a code point outside ASCII are `unsupported` (rule 1); a name
// longer than 4096 bytes is `limit_exceeded` (§7.7). An identifier with a
// special meaning (`final`) is an identifier.
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
  note("plain: " + OUTCOME(identifier("new_name")));
  note("final: " + OUTCOME(identifier("final")));
  note("keyword: " + OUTCOME(identifier("class")));
  note("alternative token: " + OUTCOME(identifier("and")));
  note("two tokens: " + OUTCOME(identifier("a b")));
  note("empty: " + OUTCOME(identifier("")));
  note("qualified: " + OUTCOME(identifier("n::x")));
  note("digit: " + OUTCOME(identifier("1a")));
  note("not UTF-8: " + OUTCOME(identifier("\xff")));
  note("dollar: " + OUTCOME(identifier("a$")));
  note("outside ASCII: " + OUTCOME(identifier("\xc3\xa9")));
  note("4096 bytes: " + OUTCOME(identifier(std::string(4096, 'x'))));
  note("4097 bytes: " + OUTCOME(identifier(std::string(4097, 'x'))));
}
