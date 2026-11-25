#include "side.h"
// WP-156 fix round 1 (W3, W10; `spec/digest.md` §2.4 "What Kiln does not
// keep"): a declaration whose type a constrained placeholder deduced. Kiln
// replaces the declared type by the deduced one and keeps the
// type-constraint nowhere, so `make` in `Same<decltype(make(0))>` cannot
// be read. The translation unit's type arena holds such a constraint with
// an operand, so every declaration whose type a placeholder deduced is
// indeterminate in its operand unit (`kiln/placeholder-constraint`),
// `auto w` and the lambda's call operator included, and so is a `new`.
namespace lib { int make(int); }
using lib::make;
template <class T, class U> concept Same = __is_same(T, U);
Same<decltype(make(0))> auto v = 1;
Same<decltype(make(0))> auto g() { return 1; }
auto w = 2;
int h() { return [] { return 3; }(); }
int *n = new int(4);
int k = 5;
