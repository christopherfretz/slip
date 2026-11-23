// spec: §6.1 rule 8
// `lookup(scope, identifier)` in a specialisation of a class template
// (spec/api.md §6.1 rule 8), which `lookup` of a qualified name never
// reaches (rule 5). The implicit instantiation `W<long>` is the parent of
// the referent of `x.a`, a member of the instantiation (§5.5). It is a
// scope (no `wrong_kind`), and its members are those written in its
// definition: an instantiation has none written (rule 5, "no written
// declaration"), so nothing is found in it, not even `a`; the primary
// template's members are found in the template.
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
  info w = one("W");
  for (info r : references_to(one("W::a"))) {
    info s = parent_of(referent_of(r));
    if (s == w) continue;
    note("W<long>'s a: " + num(lookup(s, "a").size()));
    note("W<long>'s c: " + num(lookup(s, "c").size()));
    note("W<long>'s b: " + num(lookup(s, "b").size()));
    note("W<long>'s a is W::a: " + std::string(lookup(s, "a") == lookup(w, "a") ? "yes" : "no"));
    for (info e : lookup(s, "c")) report(e, severity::note, "found c");
  }
  note("W's a: " + num(lookup(w, "a").size()));
  note("W's b: " + num(lookup(w, "b").size()));
}
