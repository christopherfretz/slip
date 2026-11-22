#include "side.h"
// WP-035, digest-kinds §7 `_` `construct-overload-added`: an added constructor that wins changes the construction.
struct S { S(long);
#ifdef B
  S(int);
#endif
};
S v(1);
