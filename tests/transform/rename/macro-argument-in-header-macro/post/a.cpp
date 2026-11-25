#include "m.h"
namespace a { int g(int x) { return x; } }
int u() { return WRAP(a::g(1)) + OUTER(a::g(2)); }
