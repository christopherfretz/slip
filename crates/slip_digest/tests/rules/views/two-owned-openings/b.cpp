#include "dep.h"
namespace acme { int g(int); }
namespace acme { int h(int); }
int u() { return acme::g(1) + acme::h(1); }
