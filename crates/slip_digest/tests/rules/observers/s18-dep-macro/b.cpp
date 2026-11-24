#define USE_DEP 1
#ifdef USE_DEP
#include "dep_m.h"
#else
#include "own_m.h"
#endif
namespace fresh { THING }
int a = sizeof(fresh::Thing);
