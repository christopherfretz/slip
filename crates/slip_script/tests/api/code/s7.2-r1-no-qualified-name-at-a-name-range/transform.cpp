// spec: §7.2 rule 1
// A spelling of an entity with no qualified name (spec/api.md §7.1 rule 4)
// is made, and at the name range of a reference it takes the sibling form
// of §7.2 rule 1: the entity's identifier, when its parent is the parent
// of the origin of the referent (§5.5). A member access names its member
// with no qualifier, so its name range is its extent: where the entity is
// no sibling the place takes rule 2, and `replace` throws `unsupported`
// (§7.1 rule 4). One of each kind: a member of a class template (`W::g`),
// a member of an explicit specialisation (`W<char>::g`), an entity
// declared in an unnamed class (`A::v`), an entity whose only declarations
// are friend declarations (`other_friend`). The condition compares the
// pattern's parent: `W::g` is a sibling of the origin `W::f` of the
// referents of `w.f()` (`W<int>`) and `l.f()` (`W<long>`), and not of
// `W<char>::f`, which is its own origin; `W<char>::g` is a sibling of that
// one only. The instantiation's own member `W<int>::g` (the referent of
// `w.g()`) is a sibling of nothing written, its own reference included:
// its parent is the instantiation. At the name range of the qualified
// `W<int>::k()`, no reference's extent, the qualifier stays: `W::h` is
// written `h` after it, and a non-sibling is `invalid_argument` there.
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

consteval std::string at_name(info r, info e) {
  return OUTCOME(replace(name_range_of(r), spell(e)));
}

consteval void transform() {
  std::vector<info> wf = references_to(one("W::f"));  // w.f(), l.f()
  std::vector<info> args = arguments_as_written(calls_to(one("take"))[0]);
  info cf = callee_reference_of(args[0]);               // c.f()
  info wg = references_to(one("W::g"))[0];              // w.g()
  info pattern_g = one("W::g");
  info char_g = callee_of(args[1]);                     // W<char>::g
  info int_g = referent_of(wg);                         // W<int>::g
  note("W::g at w.f(): " + at_name(wf[0], pattern_g));
  note("W::g at l.f(): " + at_name(wf[1], pattern_g));
  note("W::g at c.f(): " + at_name(cf, pattern_g));
  note("W<char>::g at c.f(): " + at_name(cf, char_g));
  note("W<char>::g at w.f(): " + at_name(wf[0], char_g));
  note("W<int>::g at w.f(): " + at_name(wf[0], int_g));
  note("W<int>::g at l.f(): " + at_name(wf[1], int_g));
  note("W<int>::g at w.g(): " + at_name(wg, int_g));
  note("A::v at a.u: " + at_name(references_to(one("A::u"))[0], one("A::v")));
  note("other_friend at only_friend(fr): " +
       at_name(references_to(one("only_friend"))[0], one("other_friend")));
  info wk = references_to(one("W::k"))[0];              // W<int>::k()
  note("W::h at the name of W<int>::k(): " + at_name(wk, one("W::h")));
  note("W<int>::g at the name of W<int>::k(): " + at_name(wk, int_g));
  note("A::v at the name of W<int>::k(): " + at_name(wk, one("A::v")));
}
