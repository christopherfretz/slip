// spec: §7.1 rule 4
// `spell(e)` of entities that a qualified name without template arguments
// names (spec/api.md §7.1 rule 4 lists the others): a function, a class, a
// static data member, an enumerator of a scoped enumeration, a namespace.
// Each replaces an argument of `take(1, 2, 3, 4, 5)`, an expression and not
// a reference, so §7.2 rule 1 does not apply and the text is the fully
// qualified name (§7.2 rule 2).
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
  std::vector<info> args = arguments_as_written(calls_to(one("take"))[0]);
  std::string_view names[] = {"n::f", "n::C", "n::C::sm", "n::C::E::e1", "n"};
  for (unsigned i = 0; i < 5; ++i) replace(extent_of(args[i]), spell(one(names[i])));
}
