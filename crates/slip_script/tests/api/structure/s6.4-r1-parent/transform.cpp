// spec: §6.4 rule 1
// `parent_of` (spec/api.md §6.4 rule 1): the entity whose scope the entity
// is a member of: a namespace, a class, an enumeration (for an enumerator,
// also one lookup finds through the enumeration's transparency), a function
// (for a parameter). Inline and unnamed namespaces are parents like any
// other. A function first declared by a friend declaration is a member of
// the namespace (§6.1 rule 3). Null for the global namespace. Throws
// `wrong_kind` for a declaration.
#include <slip/meta.h>

using namespace slip;

consteval std::string name(info e) {
  if (!e) return "null";
  if (e == lookup("::")[0]) return "the global namespace";
  std::string id = identifier_of(e);
  return id.empty() ? "an unnamed " + std::string(is_namespace(e) ? "namespace" : "entity") : id;
}

consteval void transform() {
  for (std::string_view q :
       {"::", "n", "n::S", "n::S::m", "n::S::A", "n::S::E::A", "n::S::In", "n::C::B", "n::f",
        "n::x", "n::i", "n::y", "n::g"})
    report(severity::note, std::string(q) + ": parent " + name(parent_of(lookup(q)[0])));
  report(severity::note,
         "the parameter p: parent " + name(parent_of(parameters_of(lookup("n::f")[0])[0])));
  report(severity::note, parent_of(parent_of(lookup("n::y")[0])) == lookup("n")[0]
                             ? "the unnamed namespace: parent n"
                             : "the unnamed namespace: another parent");
  try {
    (void)parent_of(declarations_of(lookup("n::S")[0])[0]);
    report(severity::note, "a declaration: no throw");
  } catch (const error& e) {
    report(severity::note, e.code() == errc::wrong_kind ? "a declaration: wrong_kind"
                                                       : "a declaration: another errc");
  }
}
