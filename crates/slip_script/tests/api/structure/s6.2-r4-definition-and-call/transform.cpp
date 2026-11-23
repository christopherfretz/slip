// spec: §6.2 rule 4
// `is_definition` is true of a declaration that is a definition
// ([basic.def]); `is_call` is true of a call expression in the sense of
// §6.7 (spec/api.md §6.2 rule 4): of the two calls of `g`, not of the
// argument `1`, nor of an entity or a declaration. An entity is not a
// definition, though it has one: the predicate is of declarations. Each declaration is
// reported at its name.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"S", "f", "v", "T", "U", "n", "E"})
    for (info d : declarations_of(lookup(name)[0]))
      report(name_range_of(d), severity::note,
             is_definition(d) ? "a definition" : "not a definition");
  info g = lookup("g")[0];
  for (info c : calls_to(g)) {
    report(extent_of(c), severity::note, is_call(c) ? "a call" : "not a call");
    for (info a : arguments_as_written(c))
      report(extent_of(a), severity::note, is_call(a) ? "argument: a call" : "argument: not a call");
  }
  report(severity::note, std::string("g: ") + (is_call(g) ? "a call" : "not a call"));
  report(severity::note, std::string("the entity f: ") +
                             (is_definition(lookup("f")[0]) ? "a definition" : "not a definition"));
  report(severity::note, std::string("the declaration of g: ") +
                             (is_call(declarations_of(g)[0]) ? "a call" : "not a call"));
}
