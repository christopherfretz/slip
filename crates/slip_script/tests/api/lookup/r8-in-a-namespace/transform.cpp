// spec: §6.1 rule 8
// `lookup(scope, identifier)` (spec/api.md §6.1 rule 8) in a namespace: the
// members declared with the identifier, each function of an overload set
// one entity, the members of an inline and of an unnamed namespace in it
// included (rule 3), and what `lookup` of the qualified name gives. A
// member of another scope with that identifier is not one. Each entity
// found is reported at its first declaration.
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

consteval void found(info scope, std::string_view id) {
  std::vector<info> es = lookup(scope, id);
  note(std::string(id) + ": " + num(es.size()));
  for (info e : es) report(e, severity::note, "found " + std::string(id));
}

consteval void transform() {
  info n = one("n");
  found(n, "f");
  found(n, "g");
  found(n, "h");
  found(n, "S");
  found(n, "missing");
  note(lookup(n, "f") == lookup("n::f") ? "f: as lookup(\"n::f\")" : "f: not as lookup");
  info global = lookup("::")[0];
  found(global, "f");
  note(lookup(global, "f") == lookup("::f") ? "::f: as lookup(\"::f\")" : "::f: not as lookup");
}
