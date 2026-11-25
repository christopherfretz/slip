#include "h.h"
#include "./h.h"
namespace a { int f(int x) { return x; } }
int u() { return a::f(1); }
