// spec: §6.6 rule 5
// `source_text_of(r)`: the bytes of `r` (spec/api.md §6.6 rule 5, §5.3
// rule 1: the bytes of the file, not transcoded): a string literal holding
// UTF-8, and a qualified reference written with a blank inside. Throws
// `invalid_handle` for a null range.
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  info call = calls_to(lookup("f")[0])[0];
  report(severity::note, "the argument: " + source_text_of(extent_of(arguments_as_written(call)[0])));
  report(severity::note,
         "the reference: " + source_text_of(extent_of(references_to(lookup("n::v")[0])[0])));
  try {
    (void)source_text_of(range());
    report(severity::note, "a null range: no throw");
  } catch (const error& e) {
    report(severity::note, e.code() == errc::invalid_handle ? "a null range: invalid_handle"
                                                           : "a null range: another errc");
  }
}
