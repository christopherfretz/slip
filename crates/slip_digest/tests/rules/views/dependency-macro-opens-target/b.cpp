#include "dep.h"
ACME_BEGIN int g(int); ACME_END
int u() { return acme::g(1); }
