#include "side.h"
// §5.6 FriendFormFlags `0` (out, Kiln WP-3164): a friend's qualifier written with and without the leading `::` (`FriendFormFlags` GLOBAL).
namespace n { void f(); }
struct S {
#ifdef B
  friend void ::n::f();
#else
  friend void n::f();
#endif
};
