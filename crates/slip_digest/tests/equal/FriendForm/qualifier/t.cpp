#include "side.h"
// §5.6 FriendForm `qualifier` (out, Kiln WP-3164): a friend's qualifier written through a namespace alias.
namespace n { void f(); }
namespace m = n;
struct S {
#ifdef B
  friend void m::f();
#else
  friend void n::f();
#endif
};
