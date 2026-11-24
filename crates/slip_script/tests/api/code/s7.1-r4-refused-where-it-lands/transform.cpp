// spec: §7.1 rule 4
// A spelling of an entity with no qualified name without template
// arguments (spec/api.md §7.1 rule 4) is made, and refused where it lands
// at a place that takes the fully qualified spelling (§7.2 rule 2): each
// kind replaces an argument of `take(...)`, an expression and no
// reference, alone and inside `cat` beside an identifier piece. `cat`
// makes the code (it has no place); `replace` throws `unsupported`, the
// error `spell` gave before, and its message names the entity. A refused
// request records nothing. The kinds: a specialisation (the callee of
// `tf(1)`), a member of an instantiation (the callee of `w.g()`), a member
// of an explicit specialisation (`W<char>::g`, the callee of the argument
// of `probe(c.g())`), a member of a class template (`W::g`), an entity
// declared in an unnamed class (`A::u`), an entity whose only
// declarations are friend declarations.
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

// What a call did: "returned", or the message of the error it threw.
#define MESSAGE(...)                    \
  [&]() consteval -> std::string {      \
    try {                               \
      (void)(__VA_ARGS__);              \
      return "returned";                \
    } catch (const error& e) {          \
      return e.message();               \
    }                                   \
  }()

consteval void transform() {
  std::vector<info> args = arguments_as_written(calls_to(one("take"))[0]);
  info kinds[] = {
      callee_of(calls_to(one("tf"))[0]),
      callee_of(calls_to(one("W::g"))[0]),
      callee_of(arguments_as_written(calls_to(one("probe"))[0])[0]),
      one("W::g"),
      one("A::u"),
      one("only_friend"),
  };
  std::string_view names[] = {
      "specialisation",        "member of an instantiation", "member of an explicit specialisation",
      "member of a class template", "in an unnamed class",  "only friend declarations",
  };
  for (unsigned i = 0; i < 6; ++i) {
    range at = extent_of(args[i]);
    std::string what(names[i]);
    note(what + ", spell: " + OUTCOME(spell(kinds[i])));
    note(what + ", cat: " + OUTCOME(cat(spell(kinds[i]), identifier("x"))));
    note(what + ", replace: " + OUTCOME(replace(at, spell(kinds[i]))));
    note(what + ", replace with cat: " +
         OUTCOME(replace(at, cat(identifier("x"), spell(kinds[i])))));
    note(what + ", the message: " + MESSAGE(replace(at, spell(kinds[i]))));
  }
}
