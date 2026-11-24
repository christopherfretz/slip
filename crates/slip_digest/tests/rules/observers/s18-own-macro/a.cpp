#ifdef USE_DEP
#include "dep_m.h"
#else
#include "own_m.h"
#endif
namespace old { THING }
int a = sizeof(old::Thing);
