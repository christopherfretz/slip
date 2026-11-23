// spec: §7.4 rule 3
// The place of an `info` (spec/api.md §7.4 rule 3): for a reference or an
// expression its extent; for a declaration its name; for an entity the
// name of its first written declaration. The reference from `ID(g)` is a
// whole expansion: its place is the invocation, its own. The reference `g`
// that `CALL_G` writes is not in tier 0 or 1: the report is attached to the
// invocation that produced it and says so (`attached`). The callee of
// `tf(4)` is a specialisation, an entity with no written declaration: no
// place.
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
  info g = one("g");
  std::vector<info> refs = references_to(g);
  std::vector<info> calls = calls_to(g);
  report(g, severity::note, "entity");
  report(declarations_of(g)[0], severity::note, "declaration");
  report(refs[0], severity::note, "reference");
  report(calls[0], severity::note, "expression");
  report(refs[1], severity::note, "whole expansion");
  report(refs[2], severity::note, "replacement list");
  report(callee_of(calls_to(one("tf"))[0]), severity::note, "no written declaration");
}
