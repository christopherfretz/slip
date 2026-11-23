// spec: §7.1 rule 4
// `spell(e)` throws wrong_kind if `e` is not an entity (spec/api.md §7.1
// rule 4): a declaration, a reference, an expression. A null `info` is
// invalid_handle (§4, the table), as for every function.
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
  info f = one("f");
  info call = calls_to(f)[0];
  note("spell(declaration): " + OUTCOME(spell(declarations_of(f)[0])));
  note("spell(reference): " + OUTCOME(spell(references_to(f)[0])));
  note("spell(expression): " + OUTCOME(spell(call)));
  note("spell(null): " + OUTCOME(spell(info{})));
  note("spell(entity): " + OUTCOME(spell(f)));
}
