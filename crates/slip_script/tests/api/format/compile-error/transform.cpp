// spec: §11
// A transform that does not compile (spec/api.md §11 rule 3, §2.1 rule 5):
// the error's name. The subject is not parsed (§2.1 rule 5), so its error is
// never seen.
#include <slip/meta.h>

consteval void transform() {
  this is not C++;
}
