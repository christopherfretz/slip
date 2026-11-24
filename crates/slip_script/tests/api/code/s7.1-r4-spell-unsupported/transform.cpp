// spec: §7.1 rule 4
// `spell(e)` throws unsupported for an entity that is never spelled
// (spec/api.md §7.1 rule 4): a parameter, an entity with no identifier
// (the unnamed namespace, the parent of `in_unnamed`). An entity declared
// in a function body (no key) is not reached by any query of version 0.
// An entity with an identifier and no qualified name is spelled:
// `code/s7.1-r4-spell-no-qualified-name`.
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
  note("parameter: " + OUTCOME(spell(parameters_of(one("P::f"))[0])));
  note("no identifier: " + OUTCOME(spell(parent_of(one("in_unnamed")))));
}
