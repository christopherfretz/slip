#include "side.h"
#include "new_dep.h"
#include "ro_inc.h"
#include "own.h"
#ifdef B
int u() { return acme::g(1); }
#else
int u() { return legacy::g(1); }
#endif
