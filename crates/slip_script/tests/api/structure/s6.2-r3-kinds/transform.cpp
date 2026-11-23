// spec: §6.2 rule 3
// The kind predicates (spec/api.md §6.2 rule 3) are true only of entities,
// as the table says: a namespace, the global one included; a class, an
// enumeration, a type alias are types; a function, a member function
// included, and not a function template; a variable or a data member and
// not a parameter; a template of any kind; a function parameter. An
// enumerator and a concept satisfy none (the sentence after the table). A
// declaration satisfies none.
#include <slip/meta.h>

using namespace slip;

consteval std::string kinds(info i) {
  std::string s;
  if (is_namespace(i)) s += " namespace";
  if (is_type(i)) s += " type";
  if (is_function(i)) s += " function";
  if (is_variable(i)) s += " variable";
  if (is_template(i)) s += " template";
  if (is_parameter(i)) s += " parameter";
  return s.empty() ? " none" : s;
}

consteval void transform() {
  for (std::string_view name : {"::", "n", "n::S", "n::S::m", "n::S::mf", "n::E", "n::A",
                                "n::Al", "n::Td", "n::f", "n::tf", "n::TC", "n::tv", "n::TA",
                                "n::Co", "n::v"})
    report(severity::note, std::string(name) + ":" + kinds(lookup(name)[0]));
  info f = lookup("n::f")[0];
  report(severity::note, "the parameter p:" + kinds(parameters_of(f)[0]));
  report(severity::note, "the declaration of n::f:" + kinds(declarations_of(f)[0]));
  report(severity::note, "the declaration of n::S:" + kinds(declarations_of(lookup("n::S")[0])[0]));
}
