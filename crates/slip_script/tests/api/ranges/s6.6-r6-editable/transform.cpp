// spec: §6.6 rule 6
// `is_editable(r)` (spec/api.md §6.6 rule 6): true exactly when `r` is not
// null and its file is owned. In a script test every file of the subject is
// owned unless the test says `// owned: no` (spec/api.md §11): a range in
// the main file and one in a header are editable, the null range is not.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("f")[0];
  range call = extent_of(calls_to(f)[0]);
  range decl = name_range_of(declarations_of(f)[0]);
  report(call, severity::note, is_editable(call) ? "editable" : "not editable");
  report(decl, severity::note, is_editable(decl) ? "editable" : "not editable");
  report(severity::note, is_editable(range()) ? "null: editable" : "null: not editable");
}
