#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `operator_new`: a class's own allocation function added.
struct N { int i;
#ifdef B
  static void *operator new(decltype(sizeof 0));
#endif
};
N *v = new N;
