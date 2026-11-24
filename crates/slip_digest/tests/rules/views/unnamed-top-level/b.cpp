#include "dep.h"
namespace { namespace acme { int g(int x) { return x; } } }
int u() { return acme::g(1); }
