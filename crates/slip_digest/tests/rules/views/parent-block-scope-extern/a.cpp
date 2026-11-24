#include "dep.h"
int acme::g() { extern int bx; return bx; }
int u() { return acme::g(); }
