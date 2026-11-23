#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `InitListExpr.b`: the array filler alone changes (the default constructor it calls).
struct E {
#ifdef B
  E(int = 0);
#else
  E();
#endif
  E(const E &);
};
extern E e0;
#ifdef B
E v[2] = {e0};
#else
E v[2] = {e0};
#endif
