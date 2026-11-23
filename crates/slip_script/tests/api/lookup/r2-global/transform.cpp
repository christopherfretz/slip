// spec: §6.1 rule 2
// `::` alone names the global namespace (spec/api.md §6.1 rule 2): one
// entity, a namespace, the parent of a namespace and of a variable declared
// at global scope, and itself with no parent (§6.4 rule 1).
#include <slip/meta.h>

using namespace slip;

consteval void say(bool b, std::string_view what) {
  report(severity::note, std::string(b ? "yes: " : "no: ") + std::string(what));
}

consteval void transform() {
  std::vector<info> g = lookup("::");
  say(g.size() == 1, "lookup(\"::\") gives one entity");
  say(is_namespace(g[0]), "it is a namespace");
  say(parent_of(lookup("n")[0]) == g[0], "it is the parent of n");
  say(parent_of(lookup("y")[0]) == g[0], "it is the parent of y");
  say(!parent_of(g[0]), "it has no parent");
}
