// spec: §7.2 rule 2
// The fully qualified spelling (spec/api.md §7.2 rule 2): `::`, then the
// identifiers of the enclosing scopes and of the entity, separated by
// `::`. Unnamed namespaces are skipped; inline namespaces are named. Each
// spelling replaces an argument of `take`, an expression, so rule 1 does
// not apply.
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
  replace(extent_of(args[0]), spell(one("m::f")));
  replace(extent_of(args[1]), spell(one("n::g")));
}
