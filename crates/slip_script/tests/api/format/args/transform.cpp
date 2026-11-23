// spec: §11
// args: -DMODE=2 -UUNSET -DUNSET
// The author's options of the header line (spec/api.md §11 rule 2, §2.1
// rule 2), in the order given: MODE is 2, UNSET is defined last.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
#if MODE == 2
  report(severity::note, "MODE is 2");
#else
  report(severity::note, "MODE is not 2");
#endif
#ifdef UNSET
  report(severity::note, "UNSET is defined");
#endif
}
