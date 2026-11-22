#include "side.h"
// WP-035, digest-kinds §7 `NewExtra` `operator_delete`: the deallocation function a throwing initialisation calls.
struct N { N(); static void *operator new(decltype(sizeof 0));
#ifdef B
  static void operator delete(void *);
#endif
};
N *v = new N;
