// spec: §6.7 rule 5
// `arguments_as_written(call)` (spec/api.md §6.7 rule 5): the expressions
// written between the parentheses, in order. A default argument is not
// written and is not included (`f(1)` has one argument). The object of
// `s.m(3, 4)` is not an argument, nor is the object of a function with an
// explicit object parameter, written before the callee (`s.x(5)`) or
// implicit (`x(3)` in a member function). Each call is reported at its
// extent with the text of its arguments.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (std::string_view name : {"n::f", "n::g", "n::S::m", "n::S::x"})
    for (info c : calls_to(lookup(name)[0])) {
      std::string s = std::string(name) + ":";
      for (info a : arguments_as_written(c)) s += " [" + source_text_of(extent_of(a)) + "]";
      report(extent_of(c), severity::note, s);
    }
}
