// spec: §11
// steps: 10000
// The step budget of the header line (spec/api.md §11 rule 2, §2.5): a loop
// that needs more fails the run, `step-budget`.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  unsigned long n = 0;
  for (unsigned long i = 0; i < 1000000; ++i) n += i;
  if (n) report(severity::note, "the loop ended");
}
