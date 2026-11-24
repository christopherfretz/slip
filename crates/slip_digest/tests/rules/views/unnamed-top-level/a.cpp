#include "dep.h"
namespace { namespace legacy { int g(int x) { return x; } } }
int u() { return legacy::g(1); }
