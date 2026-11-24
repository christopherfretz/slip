#include "dep.h"
namespace legacy { namespace { int g(int x) { return x; } } }
int u() { return legacy::g(1); }
