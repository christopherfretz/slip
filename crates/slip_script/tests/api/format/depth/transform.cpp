// spec: §11
// depth: 32
// The depth budget of the header line (spec/api.md §11 rule 2, §2.5): a
// recursion of 100 calls fails the run, `depth-limit`.
#include <slip/meta.h>

using namespace slip;

consteval unsigned down(unsigned n) { return n == 0 ? 0 : 1 + down(n - 1); }

consteval void transform() {
  if (down(100) == 100) report(severity::note, "returned");
}
