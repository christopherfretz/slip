// spec: §6.6 rule 4
// `extent_of` returns null when the tokens are not in tier 0 or 1 (a call
// made by a macro's replacement list, inside parentheses that are also the
// replacement list's, so that the call is not the whole expansion), and
// that is not an error
// (spec/api.md §6.6 rule 4). It throws `wrong_kind` for an entity and
// `unsupported` for a declaration.
#include <slip/meta.h>

using namespace slip;

consteval std::string outcome(info i) {
  try {
    return extent_of(i) ? "a range" : "null";
  } catch (const error& e) {
    return e.code() == errc::wrong_kind    ? "wrong_kind"
           : e.code() == errc::unsupported ? "unsupported"
                                           : "another errc";
  }
}

consteval void transform() {
  info f = lookup("f")[0];
  std::vector<info> calls = calls_to(f);
  report(severity::note, "the call from the macro: " + outcome(calls[0]));
  report(severity::note, "the call in the file: " + outcome(calls[1]));
  report(severity::note, "an entity: " + outcome(f));
  report(severity::note, "a declaration: " + outcome(declarations_of(f)[0]));
}
