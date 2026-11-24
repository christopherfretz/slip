#include "dep.h"
namespace acme { int old(int); }
namespace acme { int g(int); }
int u() { return acme::old(1) + acme::g(1); }
