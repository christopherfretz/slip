#include "f.h"
namespace a { int f(int x) { return x; } }
int u() { return a::f(1); }
