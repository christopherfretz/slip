// spec: §5.1 rule 1
// The sort of an `info` is a property of the value (spec/api.md §5.1 rule 1,
// §6.2): an entity, one of its declarations; and the kinds of entities.
#include <slip/meta.h>

using namespace slip;

consteval std::string kinds(info i) {
  std::string s;
  if (is_entity(i)) s += " entity";
  if (is_declaration(i)) s += " declaration";
  if (is_reference(i)) s += " reference";
  if (is_expression(i)) s += " expression";
  if (is_namespace(i)) s += " namespace";
  if (is_type(i)) s += " type";
  if (is_function(i)) s += " function";
  if (is_variable(i)) s += " variable";
  if (is_template(i)) s += " template";
  if (is_parameter(i)) s += " parameter";
  if (is_definition(i)) s += " definition";
  return s;
}

consteval void transform() {
  for (std::string_view name : {"n", "n::S", "n::A", "n::f", "n::v", "n::T"}) {
    info e = lookup(name)[0];
    report(severity::note, std::string(name) + ":" + kinds(e));
    for (info d : declarations_of(e))
      report(severity::note, std::string(name) + " declaration:" + kinds(d));
  }
  info p = parameters_of(lookup("n::f")[0])[0];
  report(severity::note, "the parameter of n::f:" + kinds(p));
}
