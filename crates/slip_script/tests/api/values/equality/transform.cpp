// spec: §5.1 rule 2
// Two `info` are equal exactly when they denote the same thing of the same
// sort (spec/api.md §5.1 rule 2): one entity found twice, by two names and
// as a parent; never equal to its own declaration; two entities differ.
#include <slip/meta.h>

using namespace slip;

consteval void say(bool b, std::string_view what) {
  report(severity::note, std::string(b ? "yes: " : "no: ") + std::string(what));
}

consteval void transform() {
  info f = lookup("lib::f")[0];
  info again = lookup("lib::f")[0];
  info global = lookup("::lib::f")[0];
  info g = lookup("lib::g")[0];
  info lib = lookup("lib")[0];
  info d = declarations_of(f)[0];
  say(f == again, "lib::f found twice is one entity");
  say(f == global, "lib::f and ::lib::f are one entity");
  say(parent_of(f) == lib, "the parent of lib::f is lib");
  say(f == d, "lib::f is its declaration");
  say(entity_of(d) == f, "the entity of the declaration is lib::f");
  say(f == g, "lib::f is lib::g");
  say(f != g, "lib::f is not lib::g");
  say(declarations_of(f)[0] == d, "one declaration found twice is one declaration");
}
