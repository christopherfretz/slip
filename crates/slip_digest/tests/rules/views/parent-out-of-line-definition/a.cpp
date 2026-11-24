#include "dep.h"
int acme::g() { return 1; }
int u() { return acme::g(); }
