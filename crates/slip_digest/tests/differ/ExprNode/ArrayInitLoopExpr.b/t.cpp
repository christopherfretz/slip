#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `ArrayInitLoopExpr.b`: the element initialiser: the same array, another copy constructor.
struct S {
  S();
  S(const S &);
#ifdef B
  S(S &);
#endif
};
S arr[2];
auto [p, q] = arr;
