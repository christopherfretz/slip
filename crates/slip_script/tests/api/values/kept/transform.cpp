// spec: §5.2 rule 1
// A value is meaningful for the whole run (spec/api.md §5.2 rule 1): one
// kept across a failed call, caught (§4 rule 2), is still live and still
// the same; a function that throws has had no effect (§4 rule 3).
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("lib::f")[0];
  try {
    report(info(), severity::note, "never recorded: the call throws");
  } catch (const error&) {
  }
  try {
    (void)lookup("lib::");
  } catch (const error& e) {
    report(severity::note, e.code() == errc::invalid_argument ? "lib:: is invalid_argument"
                                                              : "lib:: is another error");
  }
  report(severity::note, "after two failed calls: " + display_string_of(f));
  report(severity::note, f == lookup("lib::f")[0] ? "still the same entity" : "another entity");
}
