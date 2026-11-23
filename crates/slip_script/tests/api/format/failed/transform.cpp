// spec: §11
// A failed run (spec/api.md §11 rule 3): the kind and the diagnostic's first
// line. A slip::error leaves transform(): `uncaught`, with the refusal.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  report(severity::note, "discarded with the run (§2.8 rule 1)");
  (void)identifier_of(info());
}
