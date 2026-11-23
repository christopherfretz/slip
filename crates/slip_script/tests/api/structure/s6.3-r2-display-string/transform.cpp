// spec: §6.3 rule 2
// `display_string_of` accepts every live `info` (spec/api.md §6.3 rule 2):
// an entity, a parameter, a declaration, a reference, a call and an
// argument. Its text is not specified, so this test reports only that it
// answered, and that two calls in one run answered the same text (§2.7
// rule 5).
#include <slip/meta.h>

using namespace slip;

consteval void check(std::string_view what, info i) {
  try {
    std::string a = display_string_of(i);
    std::string b = display_string_of(i);
    report(severity::note, std::string(what) + (a == b ? ": answered, the same twice"
                                                       : ": answered, differently twice"));
  } catch (const error&) {
    report(severity::note, std::string(what) + ": threw");
  }
}

consteval void transform() {
  info f = lookup("f")[0];
  info call = calls_to(f)[0];
  check("an entity", f);
  check("a parameter", parameters_of(f)[0]);
  check("a declaration", declarations_of(f)[0]);
  check("a reference", references_to(f)[0]);
  check("a call", call);
  check("an argument", arguments_as_written(call)[0]);
}
