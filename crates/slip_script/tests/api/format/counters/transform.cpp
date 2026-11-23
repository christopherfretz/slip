// spec: §11
// counters: api.reports, api.edits
// `// counters:` (spec/api.md §11 rule 3): only the counters listed are
// rendered, in the output's order; one the output does not have is
// `absent`. Two reports, no edit.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  report(severity::note, "one");
  report(severity::note, "two");
}
