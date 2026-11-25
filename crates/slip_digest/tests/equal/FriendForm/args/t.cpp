#include "side.h"
// §5.6 FriendForm `args` (out, Kiln WP-3164): a befriended function template specialisation with its argument written and deduced.
template <class T> void h(T);
struct S {
#ifdef B
  friend void h<int>(int);
#else
  friend void h<>(int);
#endif
};
