// spec: §6.1 rule 8
// `lookup(scope, identifier)` in a class, a class template and a class
// named by a typedef for linkage purposes (spec/api.md §6.1 rules 4 and
// 8): a data member, a static member function, the enumerator of an
// unscoped member enumeration (transparent, rule 3); a member of a nested
// class is not a member of the class around it.
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

consteval void count(info scope, std::string_view what, std::string_view id) {
  note(std::string(what) + "::" + std::string(id) + ": " + num(lookup(scope, id).size()));
}

consteval void transform() {
  info s = one("n::S");
  count(s, "S", "m");
  count(s, "S", "f");
  count(s, "S", "In");
  count(s, "S", "k");
  count(s, "S", "e1");
  count(one("n::S::In"), "In", "k");
  count(one("n::C"), "C", "t");
  count(one("n::L"), "L", "u");
}
