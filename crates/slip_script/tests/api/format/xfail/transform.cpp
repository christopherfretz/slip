// spec: §11
// Previously xfail on Kiln WP-2480: since WP-148 item 7, rethrowing an
// older `slip::error` after a newer refusal reports the thrown object's
// own refusal (`spec/api.md` §2.8 rule 3).
#include <slip/meta.h>

using namespace slip;

consteval void transform() {
  try {
    (void)identifier_of(info());  // invalid_handle
  } catch (const error& older) {
    try {
      (void)lookup("not a name");  // invalid_argument
    } catch (const error&) {
    }
    throw older;
  }
}
