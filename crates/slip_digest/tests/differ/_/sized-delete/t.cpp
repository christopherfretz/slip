#include "side.h"
// WP-035, digest-kinds §7 `_` `sized-delete`.
struct C { int i;
#ifdef B
  static void operator delete(void *, decltype(sizeof 0));
#else
  static void operator delete(void *);
#endif
};
C *pc;
int v = (delete pc, 0);
