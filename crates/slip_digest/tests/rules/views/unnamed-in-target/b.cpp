#include "dep.h"
namespace acme { namespace { int g(int x) { return x; } } }
int u() { return acme::g(1); }
