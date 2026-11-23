// spec/api.md §10.2, declarations in two headers and a definition: each
// name is replaced, and the rename is declared (the rule that undoes it,
// spec/verify.md §2.1).
#include "a.h"
#include "b.h"
namespace lib {
int old_name(int x) { return x; }
}
