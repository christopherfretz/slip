// spec: §6.7 rule 7
// A reference to a function that is not the callee of a call (`&f`, `f`
// passed as an argument) is in `references_to(f)` and in no call
// (spec/api.md §6.7 rule 7). Each reference is reported at its extent,
// saying whether it is the callee's reference of a call of `f`.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("f")[0];
  std::vector<info> calls = calls_to(f);
  for (info r : references_to(f)) {
    bool callee = false;
    for (info c : calls)
      if (callee_reference_of(c) == r) callee = true;
    report(extent_of(r), severity::note, callee ? "the callee of a call" : "in no call");
  }
}
