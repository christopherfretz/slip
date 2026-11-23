// spec: §11
// A completed run (spec/api.md §11 rule 3): the run output of §7.6, one
// item per line. Two reports with no place, one that names an entity.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  for (info e : lookup("lib::f"))
    report(severity::note, display_string_of(e));
  report(severity::review, "second \"quoted\"");
}
