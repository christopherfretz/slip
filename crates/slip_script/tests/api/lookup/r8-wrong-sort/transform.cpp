// spec: §6.1 rule 8
// `lookup(scope, identifier)` takes an entity that is a scope (spec/api.md
// §6.1 rule 8): a function, a variable, a type alias, a declaration, a
// reference and an expression are refused with `wrong_kind`; a null info
// with `invalid_handle` (§6).
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
  note("function: " + OUTCOME(lookup(f, "x")));
  note("variable: " + OUTCOME(lookup(one("v"), "x")));
  note("alias: " + OUTCOME(lookup(one("A"), "x")));
  note("declaration: " + OUTCOME(lookup(declarations_of(f)[0], "x")));
  info call = calls_to(f)[0];
  note("reference: " + OUTCOME(lookup(callee_reference_of(call), "x")));
  note("expression: " + OUTCOME(lookup(call, "x")));
  note("null: " + OUTCOME(lookup(info{}, "x")));
}
