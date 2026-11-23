// spec: §11
// owned: no
// `// owned: no` (spec/api.md §11 rule 2): no file of the subject is
// editable, so the name range of `first` is not (§6.6 `is_editable`);
// format/places shows the same range editable without the line.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  range first = name_range_of(declarations_of(lookup("first")[0])[0]);
  report(first, severity::note, is_editable(first) ? "editable" : "not editable");
}
