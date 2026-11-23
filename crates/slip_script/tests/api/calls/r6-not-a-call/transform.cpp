// spec: §6.7 rule 6
// `callee_of`, `callee_reference_of` and `arguments_as_written` throw
// `wrong_kind` for an `info` that is not a call (spec/api.md §6.7 rule 6):
// an expression that is not a call (the argument `1 + 2`), an entity, a
// declaration, a reference.
#include <slip/meta.h>

using namespace slip;

#define OUTCOME(fn, value)                                                    \
  [&]() consteval -> std::string {                                            \
    try {                                                                     \
      (void)fn(value);                                                        \
      return #fn ": no throw";                                                \
    } catch (const error& e) {                                                \
      return std::string(#fn) +                                               \
             (e.code() == errc::wrong_kind ? ": wrong_kind" : ": another errc"); \
    }                                                                         \
  }()

consteval void check(std::string_view what, info i) {
  report(severity::note, std::string(what) + ", " + OUTCOME(callee_of, i));
  report(severity::note, std::string(what) + ", " + OUTCOME(callee_reference_of, i));
  report(severity::note, std::string(what) + ", " + OUTCOME(arguments_as_written, i));
}

consteval void transform() {
  info f = lookup("f")[0];
  check("an argument", arguments_as_written(calls_to(f)[0])[0]);
  check("an entity", f);
  check("a declaration", declarations_of(f)[0]);
  check("a reference", references_to(f)[0]);
}
