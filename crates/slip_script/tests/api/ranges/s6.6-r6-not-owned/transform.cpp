// spec: §6.6 rule 6
// owned: no
// `is_editable(r)` (spec/api.md §6.6 rule 6) is false for a range in a file
// that is not owned: with `// owned: no` (spec/api.md §11) no file of the
// subject is, so neither the call in the main file nor the declaration in
// the header is editable.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info f = lookup("f")[0];
  range call = extent_of(calls_to(f)[0]);
  range decl = name_range_of(declarations_of(f)[0]);
  report(call, severity::note, is_editable(call) ? "editable" : "not editable");
  report(decl, severity::note, is_editable(decl) ? "editable" : "not editable");
}
