#include "side.h"
// WP-035, digest-kinds §7 `_` `rewritten-written-order-instantiation`: the instantiation of `rewritten-written-order-in-pattern`, which calls `operator!=(Y, X)` on side A only.
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
