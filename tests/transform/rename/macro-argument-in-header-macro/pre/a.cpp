#include "m.h"
namespace a { int f(int x) { return x; } }
int u() { return WRAP(a::f(1)) + OUTER(a::f(2)); }
