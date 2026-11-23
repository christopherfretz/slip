// spec: §11
// xfail: WP-085: the subject gives no ranges yet, so name_range_of answers a null range and the run fails with invalid_handle
// Places in the rendering (spec/api.md §11 rule 3): `file:line:col-line:col`,
// columns in bytes from 1, the end being the position of the byte after the
// range (spec/landed/api/WP-090.md). The name ranges of two declarations,
// one per line, and whether the first is editable (it is: every file of the
// subject is, without `// owned: no`).
#include <slip/meta.h>

using namespace slip;

consteval range name_of(std::string_view name) {
  return name_range_of(declarations_of(lookup(name)[0])[0]);
}

consteval void transform() {
  range first = name_of("first");
  report(first, severity::note, source_text_of(first));
  report(name_of("second"), severity::review, "on the second line");
  report(severity::note, is_editable(first) ? "editable" : "not editable");
}
