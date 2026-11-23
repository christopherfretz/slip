// spec: §7.1 rule 4
// `spell(e)` throws unsupported if `e` cannot be named by a qualified name
// without template arguments (spec/api.md §7.1 rule 4), one entity of each
// kind the rule lists that a query reaches: a specialisation (the callee of
// `tf(1)`, §5.5), a member of one (the callee of `S<int>::k()`), a member
// of a class template (`S::m`), a parameter, an entity with no identifier
// (the unnamed namespace, the parent of `in_unnamed`), an entity declared
// in an unnamed class (`A::u`, a member of an anonymous union), an entity
// whose only declarations are friend declarations. An entity declared in a
// function body (no key) is not reached by any query of version 0.
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
  note("specialisation: " + OUTCOME(spell(callee_of(calls_to(one("tf"))[0]))));
  note("member of a specialisation: " + OUTCOME(spell(callee_of(calls_to(one("S::k"))[0]))));
  note("member of a class template: " + OUTCOME(spell(one("S::m"))));
  note("parameter: " + OUTCOME(spell(parameters_of(one("P::f"))[0])));
  note("no identifier: " + OUTCOME(spell(parent_of(one("in_unnamed")))));
  note("in an unnamed class: " + OUTCOME(spell(one("A::u"))));
  note("only friend declarations: " + OUTCOME(spell(one("only_friend"))));
}
