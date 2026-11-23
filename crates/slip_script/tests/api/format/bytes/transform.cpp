// spec: §11
// bytes: 4096
// The memory budget of the header line (spec/api.md §11 rule 2, §2.5): an
// allocation of more fails the run, `memory-budget`.
#include <slip/meta.h>

#include <vector>

using namespace slip;

consteval void transform() {
  std::vector<unsigned long> v(100000, 1);
  if (v.size()) report(severity::note, "allocated");
}
