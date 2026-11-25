#include "side.h"
// WP-156 fix round 1 (W3, W10; `spec/digest.md` §2.4 "What Kiln does not
// keep"): a declaration whose type a constrained placeholder deduced. Kiln
// replaces the declared type by the deduced one and keeps the
// type-constraint nowhere, so `make` in `Same<decltype(make(0))>` cannot
// be read: the declaration is indeterminate in its operand unit
// (`kiln/placeholder-constraint`), before its name or in a trailing return
// type, and so is a `new` of such a placeholder. Fix round 2 (M-2, the
// review's zz_r7): the operand's location is between the bounds around
// the declaration's name (or inside the `new`), so `auto w`, the lambda's
// call operator, `new int(4)` and `k` fail nothing.
namespace lib { int make(int); }
using lib::make;
template <class T, class U> concept Same = __is_same(T, U);
Same<decltype(make(0))> auto v = 1;
Same<decltype(make(0))> auto g() { return 1; }
auto t() -> Same<decltype(make(0))> auto { return 2; }
auto w = 2;
int h() { return [] { return 3; }(); }
int *n = new int(4);
int *n2 = new Same<decltype(make(0))> auto(4);
int k = 5;
// A class declared by an elaborated-type-specifier inside the
// type-constraint's arguments does not bound the window.
template <class T, class U, class V, class W> concept Same3 = __is_same(T, U);
Same3<decltype(make(0)), int, struct Z *> auto z3 = 1;
