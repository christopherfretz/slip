// spec: §11
// xfail: Kiln WP-2480: an uncaught slip::error is reported with the run's most recent refusal, not the one thrown (spec/api.md §2.8 rule 3)
// An `xfail` test whose rendering differs (spec/api.md §11 rule 2,
// spec/harness.md §6): XFAIL, which passes the suite. The older refusal is
// rethrown after a newer one; expect.txt holds the right report, the older
// refusal's, which the host cannot give until Kiln names the object thrown.
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
