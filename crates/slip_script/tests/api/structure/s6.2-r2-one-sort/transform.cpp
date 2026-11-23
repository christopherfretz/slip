// spec: §6.2 rule 2
// Exactly one of the four sort predicates is true of a live `info`
// (spec/api.md §6.2 rule 2): of an entity, a parameter, a declaration, a
// reference, a call and an argument.
#include <slip/meta.h>

using namespace slip;

consteval void sorts(std::string_view what, info i) {
  std::string s;
  if (is_entity(i)) s += " entity";
  if (is_declaration(i)) s += " declaration";
  if (is_reference(i)) s += " reference";
  if (is_expression(i)) s += " expression";
  report(severity::note, std::string(what) + ":" + s);
}

consteval void transform() {
  info f = lookup("f")[0];
  info call = calls_to(f)[0];
  sorts("f", f);
  sorts("the parameter a", parameters_of(f)[0]);
  sorts("the declaration of f", declarations_of(f)[0]);
  sorts("the reference to f", references_to(f)[0]);
  sorts("the call", call);
  sorts("the argument", arguments_as_written(call)[0]);
}
