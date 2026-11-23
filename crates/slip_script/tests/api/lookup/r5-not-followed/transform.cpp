// spec: §6.1 rule 5
// Lookup does not follow using-declarations, using-directives, namespace
// aliases, type aliases as scopes or base classes (spec/api.md §6.1
// rule 5). A using-declaration and a namespace alias are found as
// themselves, under their own name, and satisfy no kind predicate of §6.2
// but `is_entity`: the using-declaration `b::x` is not the variable `a::x`.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

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
  return s;
}

consteval void transform() {
  for (std::string_view name : {"b::x", "c", "c::x", "c::B", "b::B", "D::m", "E::m"}) {
    std::vector<info> found = lookup(name);
    std::string s = std::string(name) + ": " + num(found.size());
    for (info e : found) s += " (" + kinds(e) + " )";
    report(severity::note, s);
  }
  report(severity::note, lookup("b::x")[0] == lookup("a::x")[0] ? "b::x is a::x"
                                                                : "b::x is not a::x");
}
