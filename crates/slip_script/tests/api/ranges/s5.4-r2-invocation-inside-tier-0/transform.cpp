// spec: §5.4 rule 2
// In tier 0, `R` may contain whole macro invocations (spec/api.md §5.4
// rule 2): the argument `MAX(1, 2)` of a call written in file text has
// tier 0, and its extent is the invocation; so does the call.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info call = calls_to(lookup("f")[0])[0];
  report(extent_of(call), severity::note, "the call");
  report(extent_of(arguments_as_written(call)[0]), severity::note, "its argument");
}
