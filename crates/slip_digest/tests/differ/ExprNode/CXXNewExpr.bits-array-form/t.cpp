#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.bits-array-form`: alone, in a pattern: `new int[]{…}` (no array size, no function yet) against `new int{…}`.
template <class T> struct A {
#ifdef B
  static inline int *w = new int{T()};
#else
  static inline int *w = new int[]{T()};
#endif
};
