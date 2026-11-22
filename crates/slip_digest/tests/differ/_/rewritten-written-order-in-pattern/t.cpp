#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-written-order-in-pattern`: `CXXRewrittenBinaryOperator.b`: the same semantic form, the operands written in the other order; instantiation rebuilds from the written order and finds another function.
struct X { X(int); }; struct Y {};
bool operator==(X, Y);
template <int N> struct T {
#ifdef B
  static inline bool v = X(N) != Y{};
#else
  static inline bool v = Y{} != X(N);
#endif
};
bool operator!=(Y, X);
bool w = T<1>::v;
