#include "h.h"
#include "./h.h"
namespace a { int g(int x) { return x; } }
int u() { return a::g(1); }
